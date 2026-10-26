//! Clocks service. Spec: docs/features/clocks.md.
//! The clocks are autosaved in the `clocks` workspace. Timed clocks only move
//! when someone calls `clocks_tick` (the app does when a segment is due), so
//! the caller that ticks is the one that gets the notifications.

use serde::{Deserialize, Serialize};
use sleuth_core::clocks::{
    Clock, ClockEvent, MAX_EVERY_SECS, MAX_SEGMENTS, MIN_EVERY_SECS, valid_every, valid_segments,
};
use sleuth_core::settings::HexColor;
use sleuth_store::{Store, now};

use crate::{Result, Service, ServiceError};

const KEY: &str = "clocks";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Clocks {
    #[serde(default)]
    clocks: Vec<Clock>,
}

fn load(s: &Store) -> Result<Clocks> {
    Ok(s.workspace_get::<Clocks>(KEY)?.unwrap_or_default())
}

fn save(s: &Store, c: &Clocks) -> Result<()> {
    s.workspace_set(KEY, c)?;
    s.record_change("clocks", KEY, "update")?;
    Ok(())
}

pub(crate) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// A new clock; everything is optional.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClockNew {
    /// Default `Clock N`.
    pub name: Option<String>,
    /// 2–24, default 4.
    pub segments: Option<u32>,
    pub color: Option<String>,
    /// Advance one segment every this many seconds (10 s – 24 h).
    pub every_secs: Option<u32>,
    /// Start the timer right away.
    pub run: Option<bool>,
}

/// Changes to a clock; `None` leaves a field as it is.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClockPatch {
    pub name: Option<String>,
    pub segments: Option<u32>,
    pub filled: Option<u32>,
    /// `Some("")` goes back to the accent colour.
    pub color: Option<String>,
    pub notes: Option<String>,
    /// `Some(0)` removes the timer (filled by hand only).
    pub every_secs: Option<u32>,
    /// Replaces every event (events without an id get one).
    pub events: Option<Vec<ClockEvent>>,
}

/// An event triggered when its segment filled.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FiredEvent {
    pub segment: u32,
    pub text: String,
}

/// One clock the timer moved.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ClockTick {
    pub id: String,
    pub name: String,
    /// Segments added by this tick.
    pub added: u32,
    pub filled: u32,
    pub segments: u32,
    /// The clock filled (and its timer stopped).
    pub full: bool,
    /// Events of the segments that filled, in segment order.
    pub events: Vec<FiredEvent>,
}

fn check_segments(n: u32) -> Result<u32> {
    if valid_segments(n) {
        Ok(n)
    } else {
        Err(ServiceError::InvalidInput("a clock has 2–24 segments".into()))
    }
}

fn check_every(secs: u32) -> Result<u32> {
    if valid_every(secs) {
        Ok(secs)
    } else {
        Err(ServiceError::InvalidInput(format!(
            "the timer period is {MIN_EVERY_SECS} s to {} h",
            MAX_EVERY_SECS / 3600
        )))
    }
}

fn check_color(c: &str) -> Result<Option<String>> {
    let c = c.trim();
    if c.is_empty() {
        return Ok(None);
    }
    HexColor::parse(c)
        .map(|_| Some(c.to_ascii_lowercase()))
        .ok_or_else(|| ServiceError::InvalidInput(format!("{c} is not a #rrggbb colour")))
}

fn check_events(list: Vec<ClockEvent>) -> Result<Vec<ClockEvent>> {
    list.into_iter()
        .map(|mut e| {
            e.text = e.text.trim().to_string();
            if e.text.is_empty() {
                return Err(ServiceError::InvalidInput("an event needs some text".into()));
            }
            if !(1..=MAX_SEGMENTS).contains(&e.at) {
                return Err(ServiceError::InvalidInput(format!(
                    "an event is at segment 1–{MAX_SEGMENTS}"
                )));
            }
            e.repeat = e.repeat.filter(|&r| r > 0);
            if e.id.is_empty() {
                e.id = uuid::Uuid::new_v4().to_string();
            }
            Ok(e)
        })
        .collect()
}

fn find<'a>(c: &'a mut Clocks, id: &str) -> Result<&'a mut Clock> {
    c.clocks
        .iter_mut()
        .find(|k| k.id == id)
        .ok_or_else(|| ServiceError::NotFound(format!("clock {id}")))
}

impl Service {
    async fn clocks_mutate<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&mut Clocks) -> Result<T> + Send + 'static,
        T: Send + 'static,
    {
        self.blocking(move |s| {
            let mut c = load(s)?;
            let out = f(&mut c)?;
            save(s, &c)?;
            Ok(out)
        })
        .await
    }

    async fn clock_mutate<F>(&self, id: String, f: F) -> Result<Clock>
    where
        F: FnOnce(&mut Clock) -> Result<()> + Send + 'static,
    {
        self.clocks_mutate(move |c| {
            let k = find(c, &id)?;
            f(k)?;
            k.updated = now();
            Ok(k.clone())
        })
        .await
    }

    /// Every clock, in the order they were made.
    pub async fn clocks_list(&self) -> Result<Vec<Clock>> {
        self.blocking(|s| Ok(load(s)?.clocks)).await
    }

    pub async fn clocks_create(&self, new: ClockNew) -> Result<Clock> {
        let at = now_ms();
        self.clocks_mutate(move |c| {
            let name = match new.name.map(|n| n.trim().to_string()) {
                Some(n) if !n.is_empty() => n,
                _ => format!("Clock {}", c.clocks.len() + 1),
            };
            let segments = check_segments(new.segments.unwrap_or(4))?;
            let mut k = Clock::new(uuid::Uuid::new_v4().to_string(), name, segments, now());
            if let Some(col) = new.color {
                k.color = check_color(&col)?;
            }
            if let Some(e) = new.every_secs {
                k.set_every(Some(check_every(e)?));
                if new.run == Some(true) {
                    k.set_running(true, at);
                }
            }
            c.clocks.push(k.clone());
            Ok(k)
        })
        .await
    }

    pub async fn clocks_update(&self, id: String, patch: ClockPatch) -> Result<Clock> {
        self.clock_mutate(id, move |k| {
            if let Some(n) = patch.name {
                let n = n.trim();
                if n.is_empty() {
                    return Err(ServiceError::InvalidInput("a clock needs a name".into()));
                }
                k.name = n.to_string();
            }
            if let Some(n) = patch.segments {
                k.set_segments(check_segments(n)?);
            }
            if let Some(f) = patch.filled {
                k.set_filled(f);
            }
            if let Some(c) = patch.color {
                k.color = check_color(&c)?;
            }
            if let Some(n) = patch.notes {
                k.notes = n;
            }
            match patch.every_secs {
                Some(0) => k.set_every(None),
                Some(e) => k.set_every(Some(check_every(e)?)),
                None => {}
            }
            if let Some(list) = patch.events {
                k.events = check_events(list)?;
            }
            Ok(())
        })
        .await
    }

    /// Fills `by` segments (negative empties them), clamped to the clock.
    pub async fn clocks_advance(&self, id: String, by: i64) -> Result<Clock> {
        self.clock_mutate(id, move |k| {
            k.advance_by(by);
            Ok(())
        })
        .await
    }

    /// Empties a clock and pauses its timer.
    pub async fn clocks_reset(&self, id: String) -> Result<Clock> {
        self.clock_mutate(id, |k| {
            k.reset();
            Ok(())
        })
        .await
    }

    /// Starts or pauses one clock's timer, or (`id: None`) every timed clock's.
    /// Full clocks and clocks without a timer don't start.
    pub async fn clocks_run(&self, id: Option<String>, run: bool) -> Result<Vec<Clock>> {
        self.clocks_run_at(id, run, now_ms()).await
    }

    pub(crate) async fn clocks_run_at(
        &self,
        id: Option<String>,
        run: bool,
        at: i64,
    ) -> Result<Vec<Clock>> {
        self.clocks_mutate(move |c| {
            match id {
                Some(id) => {
                    let k = find(c, &id)?;
                    if k.auto.is_none() {
                        return Err(ServiceError::InvalidInput(format!(
                            "{} has no timer: set every_secs first",
                            k.name
                        )));
                    }
                    k.set_running(run, at);
                    k.updated = now();
                }
                None => {
                    for k in &mut c.clocks {
                        if k.running() != run && k.set_running(run, at) == run {
                            k.updated = now();
                        }
                    }
                }
            }
            Ok(c.clocks.clone())
        })
        .await
    }

    /// Deletes a clock; returns the remaining ones.
    pub async fn clocks_delete(&self, id: String) -> Result<Vec<Clock>> {
        self.clocks_mutate(move |c| {
            let n = c.clocks.len();
            c.clocks.retain(|k| k.id != id);
            if c.clocks.len() == n {
                return Err(ServiceError::NotFound(format!("clock {id}")));
            }
            Ok(c.clocks.clone())
        })
        .await
    }

    /// Adds the segments every running timer owes now and lists what moved
    /// (empty, and nothing saved, when nothing was due).
    pub async fn clocks_tick(&self) -> Result<Vec<ClockTick>> {
        self.clocks_tick_at(now_ms()).await
    }

    pub(crate) async fn clocks_tick_at(&self, at: i64) -> Result<Vec<ClockTick>> {
        self.blocking(move |s| {
            let mut c = load(s)?;
            let mut out = vec![];
            for k in &mut c.clocks {
                let before = k.filled;
                if let Some(a) = k.tick(at) {
                    k.updated = now();
                    let events = k
                        .events_reached(before, k.filled)
                        .into_iter()
                        .map(|(segment, e)| FiredEvent { segment, text: e.text.clone() })
                        .collect();
                    out.push(ClockTick {
                        id: k.id.clone(),
                        name: k.name.clone(),
                        added: a.added,
                        filled: k.filled,
                        segments: k.segments,
                        full: a.full,
                        events,
                    });
                }
            }
            if !out.is_empty() {
                save(s, &c)?;
            }
            Ok(out)
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::temp_service;

    const MIN: i64 = 60_000;

    fn ev(at: u32, repeat: Option<u32>, text: &str) -> ClockEvent {
        ClockEvent { id: String::new(), at, repeat, text: text.into() }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn tick_reports_the_events_it_reaches() {
        let (_d, svc) = temp_service();
        let k = svc
            .clocks_create(ClockNew {
                segments: Some(6),
                every_secs: Some(60),
                ..Default::default()
            })
            .await
            .unwrap();
        let k = svc
            .clocks_update(
                k.id,
                ClockPatch {
                    events: Some(vec![ev(1, Some(2), " tremor "), ev(2, Some(0), "guards")]),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert!(k.events.iter().all(|e| !e.id.is_empty()), "ids are given");
        assert_eq!((k.events[0].text.as_str(), k.events[1].repeat), ("tremor", None));
        svc.clocks_run_at(Some(k.id), true, 0).await.unwrap();
        let t = svc.clocks_tick_at(3 * MIN).await.unwrap();
        let fired: Vec<_> = t[0].events.iter().map(|f| (f.segment, f.text.as_str())).collect();
        assert_eq!(fired, [(1, "tremor"), (2, "guards"), (3, "tremor")]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn create_update_delete() {
        let (_d, svc) = temp_service();
        let a = svc.clocks_create(ClockNew::default()).await.unwrap();
        assert_eq!((a.name.as_str(), a.segments, a.filled), ("Clock 1", 4, 0));
        let b = svc
            .clocks_create(ClockNew {
                name: Some(" Guards ".into()),
                segments: Some(8),
                color: Some("#AA3311".into()),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!((b.name.as_str(), b.color.as_deref()), ("Guards", Some("#aa3311")));

        let b = svc
            .clocks_update(
                b.id.clone(),
                ClockPatch { filled: Some(5), segments: Some(4), ..Default::default() },
            )
            .await
            .unwrap();
        assert_eq!((b.segments, b.filled), (4, 4));
        let b = svc.clocks_advance(b.id, -1).await.unwrap();
        assert_eq!(b.filled, 3);

        for bad in [
            ClockPatch { segments: Some(1), ..Default::default() },
            ClockPatch { name: Some("  ".into()), ..Default::default() },
            ClockPatch { color: Some("red".into()), ..Default::default() },
            ClockPatch { every_secs: Some(5), ..Default::default() },
            ClockPatch { events: Some(vec![ev(0, None, "x")]), ..Default::default() },
            ClockPatch { events: Some(vec![ev(2, None, "  ")]), ..Default::default() },
        ] {
            assert!(svc.clocks_update(a.id.clone(), bad).await.is_err());
        }

        let left = svc.clocks_delete(a.id.clone()).await.unwrap();
        assert_eq!(left.len(), 1);
        assert!(svc.clocks_delete(a.id).await.is_err());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn timers_tick_pause_and_fill() {
        let (_d, svc) = temp_service();
        let k = svc
            .clocks_create(ClockNew {
                name: Some("Ritual".into()),
                segments: Some(3),
                every_secs: Some(600),
                ..Default::default()
            })
            .await
            .unwrap();
        let hand = svc.clocks_create(ClockNew::default()).await.unwrap();
        assert!(svc.clocks_run_at(Some(hand.id), true, 0).await.is_err(), "no timer");

        svc.clocks_run_at(Some(k.id.clone()), true, 0).await.unwrap();
        assert!(svc.clocks_tick_at(9 * MIN).await.unwrap().is_empty());
        let t = svc.clocks_tick_at(10 * MIN).await.unwrap();
        assert_eq!((t.len(), t[0].added, t[0].filled, t[0].full), (1, 1, 1, false));

        // pause all at 15 min, resume at 100: the half segment is kept
        let all = svc.clocks_run_at(None, false, 15 * MIN).await.unwrap();
        assert!(all.iter().all(|c| !c.running()));
        assert!(svc.clocks_tick_at(99 * MIN).await.unwrap().is_empty());
        svc.clocks_run_at(None, true, 100 * MIN).await.unwrap();
        let t = svc.clocks_tick_at(200 * MIN).await.unwrap();
        assert_eq!((t[0].added, t[0].filled, t[0].full), (2, 3, true));
        let list = svc.clocks_list().await.unwrap();
        assert!(!list[0].running(), "a full clock stops");

        let k = svc.clocks_reset(k.id).await.unwrap();
        assert_eq!(k.filled, 0);
        let k = svc
            .clocks_update(k.id, ClockPatch { every_secs: Some(0), ..Default::default() })
            .await
            .unwrap();
        assert!(k.auto.is_none());
    }
}
