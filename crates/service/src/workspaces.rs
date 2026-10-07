//! Autosaved UI/editor state (`key -> json`), e.g. the active campaign or a
//! sheet's contents. Keys are documented in docs/data-model.md (workspaces).

use serde_json::Value;

use crate::{Result, Service, ServiceError};

fn check_key(key: &str) -> Result<()> {
    let ok = !key.is_empty()
        && key.len() <= 64
        && key.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if ok { Ok(()) } else { Err(ServiceError::InvalidInput(format!("bad workspace key {key:?}"))) }
}

impl Service {
    pub async fn workspace_get(&self, key: String) -> Result<Option<Value>> {
        check_key(&key)?;
        self.blocking(move |s| Ok(s.workspace_get::<Value>(&key)?)).await
    }

    pub async fn workspace_set(&self, key: String, value: Value) -> Result<()> {
        check_key(&key)?;
        self.blocking(move |s| Ok(s.workspace_set(&key, &value)?)).await
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::temp_service;

    #[tokio::test]
    async fn roundtrip_and_key_validation() {
        let (_d, svc) = temp_service();
        assert_eq!(svc.workspace_get("ui.campaign".into()).await.unwrap(), None);
        svc.workspace_set("ui.campaign".into(), json!("c1")).await.unwrap();
        assert_eq!(svc.workspace_get("ui.campaign".into()).await.unwrap(), Some(json!("c1")));
        assert!(svc.workspace_set("../etc".into(), json!(1)).await.is_err());
    }
}
