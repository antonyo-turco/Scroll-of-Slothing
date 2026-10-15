//! Settings service. Spec: `docs/features/settings.md`.

use serde::Serialize;
use serde_json::{Map, Value};
use sleuth_core::settings::{Settings, SettingsErrors};

use crate::{Result, Service};

/// Result of a settings patch: the settings after applying valid keys, plus
/// per-key errors for the rejected ones.
#[derive(Debug, Clone, Serialize)]
pub struct SettingsUpdate {
    pub settings: Settings,
    pub errors: SettingsErrors,
}

impl Service {
    pub async fn settings_get(&self) -> Result<Settings> {
        self.blocking(|s| Ok(s.load_settings()?)).await
    }

    /// Applies the valid keys of `patch` and persists them; invalid keys are reported.
    pub async fn settings_set(&self, patch: Map<String, Value>) -> Result<SettingsUpdate> {
        self.blocking(move |s| {
            let mut settings = s.load_settings()?;
            let errors = settings.apply_patch(&patch);
            s.save_settings(&settings)?;
            Ok(SettingsUpdate { settings, errors })
        })
        .await
    }

    pub async fn settings_reset(&self) -> Result<Settings> {
        self.blocking(|s| Ok(s.reset_settings()?)).await
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use sleuth_core::units::PageFormat;

    use crate::temp_service;

    #[tokio::test]
    async fn set_persists_valid_keys_and_reports_invalid() {
        let (_d, svc) = temp_service();
        let patch = json!({"token_page_format": "A4", "map_square_in": -2.0});
        let up = svc.settings_set(patch.as_object().unwrap().clone()).await.unwrap();
        assert_eq!(up.settings.token_page_format, PageFormat::A4);
        assert!(up.errors.contains_key("map_square_in"));
        let again = svc.settings_get().await.unwrap();
        assert_eq!(again.token_page_format, PageFormat::A4);
        assert_eq!(again.map_square_in, 1.0);
        svc.settings_reset().await.unwrap();
        assert_eq!(svc.settings_get().await.unwrap().token_page_format, PageFormat::Letter);
    }
}
