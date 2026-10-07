//! Background jobs: long operations (exports, imports, full-res processing)
//! report progress through events and can be cancelled.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use serde::Serialize;
use serde_json::Value;

use crate::{Event, Result, Service, ServiceError};

pub type JobId = String;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Running,
    Done,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize)]
pub struct JobStatus {
    pub id: JobId,
    pub kind: String,
    pub state: JobState,
    /// 0.0 ..= 1.0
    pub progress: f32,
    pub message: String,
    pub result: Option<Value>,
    pub error: Option<String>,
}

type Entries = HashMap<JobId, (JobStatus, Arc<AtomicBool>)>;

#[derive(Default)]
pub(crate) struct Jobs {
    entries: Mutex<Entries>,
}

impl Jobs {
    fn lock(&self) -> MutexGuard<'_, Entries> {
        self.entries.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Handed to a job body to report progress and observe cancellation.
#[derive(Clone)]
pub struct JobCtx {
    id: JobId,
    cancel: Arc<AtomicBool>,
    svc: Service,
}

impl JobCtx {
    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    /// Returns `Err(Cancelled)` if cancellation was requested; call between work units.
    pub fn check(&self) -> Result<()> {
        if self.is_cancelled() { Err(ServiceError::Cancelled) } else { Ok(()) }
    }

    pub fn progress(&self, progress: f32, message: impl Into<String>) {
        let msg = message.into();
        if let Some(st) = self.svc.update_job(&self.id, |s| {
            s.progress = progress.clamp(0.0, 1.0);
            s.message = msg;
        }) {
            self.svc.emit(Event::Job(st));
        }
    }

    /// The service, for jobs that need the store or other operations.
    pub fn service(&self) -> &Service {
        &self.svc
    }
}

impl Service {
    fn update_job(&self, id: &str, f: impl FnOnce(&mut JobStatus)) -> Option<JobStatus> {
        self.inner.jobs.lock().get_mut(id).map(|(st, _)| {
            f(st);
            st.clone()
        })
    }

    /// Starts `body` on the blocking pool and returns its id immediately.
    /// Must be called from within a tokio runtime.
    pub fn spawn_job<F>(&self, kind: &str, body: F) -> JobId
    where
        F: FnOnce(JobCtx) -> Result<Value> + Send + 'static,
    {
        let id = uuid::Uuid::new_v4().to_string();
        let cancel = Arc::new(AtomicBool::new(false));
        let status = JobStatus {
            id: id.clone(),
            kind: kind.to_string(),
            state: JobState::Running,
            progress: 0.0,
            message: String::new(),
            result: None,
            error: None,
        };
        self.inner.jobs.lock().insert(id.clone(), (status.clone(), cancel.clone()));
        self.emit(Event::Job(status));

        let ctx = JobCtx { id: id.clone(), cancel, svc: self.clone() };
        let svc = self.clone();
        let job_id = id.clone();
        tokio::task::spawn_blocking(move || {
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| body(ctx)))
                .unwrap_or_else(|_| Err(ServiceError::Internal("job panicked".into())));
            if let Some(st) = svc.update_job(&job_id, |s| match outcome {
                Ok(v) => {
                    s.state = JobState::Done;
                    s.progress = 1.0;
                    s.result = Some(v);
                }
                Err(ServiceError::Cancelled) => s.state = JobState::Cancelled,
                Err(e) => {
                    s.state = JobState::Failed;
                    s.error = Some(e.to_string());
                }
            }) {
                svc.emit(Event::Job(st));
            }
        });
        id
    }

    pub fn job_status(&self, id: &str) -> Result<JobStatus> {
        self.inner
            .jobs
            .lock()
            .get(id)
            .map(|(s, _)| s.clone())
            .ok_or_else(|| ServiceError::NotFound(format!("job {id}")))
    }

    pub fn jobs_list(&self) -> Vec<JobStatus> {
        self.inner.jobs.lock().values().map(|(s, _)| s.clone()).collect()
    }

    /// Requests cancellation; the job stops at its next `check()`.
    pub fn job_cancel(&self, id: &str) -> Result<()> {
        let jobs = self.inner.jobs.lock();
        let (_, flag) = jobs.get(id).ok_or_else(|| ServiceError::NotFound(format!("job {id}")))?;
        flag.store(true, Ordering::Relaxed);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use serde_json::json;

    use super::*;
    use crate::temp_service;

    async fn wait_done(svc: &Service, id: &str) -> JobStatus {
        for _ in 0..1000 {
            let st = svc.job_status(id).unwrap();
            if st.state != JobState::Running {
                return st;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        panic!("job did not finish");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn job_reports_progress_and_result() {
        let (_d, svc) = temp_service();
        let mut rx = svc.subscribe();
        let id = svc.spawn_job("test", |ctx| {
            ctx.progress(0.5, "half");
            Ok(json!({"answer": 42}))
        });
        let st = wait_done(&svc, &id).await;
        assert_eq!(st.state, JobState::Done);
        assert_eq!(st.progress, 1.0);
        assert_eq!(st.result, Some(json!({"answer": 42})));
        let mut saw_half = false;
        while let Ok(ev) = rx.try_recv() {
            if let Event::Job(s) = ev {
                saw_half |= s.message == "half" && (s.progress - 0.5).abs() < 1e-6;
            }
        }
        assert!(saw_half, "progress event was broadcast");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn job_can_be_cancelled() {
        let (_d, svc) = temp_service();
        let id = svc.spawn_job("loop", |ctx| {
            loop {
                ctx.check()?;
                std::thread::sleep(Duration::from_millis(1));
            }
        });
        svc.job_cancel(&id).unwrap();
        assert_eq!(wait_done(&svc, &id).await.state, JobState::Cancelled);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn failing_and_panicking_jobs_are_reported() {
        let (_d, svc) = temp_service();
        let a = svc.spawn_job("fail", |_| Err(ServiceError::InvalidInput("bad".into())));
        let b = svc.spawn_job("panic", |_| panic!("boom"));
        let sa = wait_done(&svc, &a).await;
        assert_eq!(sa.state, JobState::Failed);
        assert!(sa.error.unwrap().contains("bad"));
        assert_eq!(wait_done(&svc, &b).await.state, JobState::Failed);
        assert!(svc.job_status("nope").is_err());
    }
}
