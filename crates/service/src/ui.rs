//! Remote control of the desktop UI (the in-app MCP server's `ui_*` tools).
//! Only meaningful in the process that hosts the webview, which calls
//! [`Service::attach_ui`] at startup.

use std::sync::atomic::Ordering;

use sleuth_core::settings::ROUTES;

use crate::{Event, Result, Service, ServiceError};

/// Editor view that re-opens an asset of `kind` (Python `app.py:539-561`).
pub fn editor_route(kind: &str) -> Option<&'static str> {
    match kind {
        "token" => Some("tokens.cutter"),
        "cutout" => Some("cutouts.cutter"),
        "map" => Some("maps.paginator"),
        _ => None,
    }
}

impl Service {
    /// Marks this process as hosting the UI, enabling the `ui_*` calls.
    pub fn attach_ui(&self) {
        self.inner.ui_attached.store(true, Ordering::SeqCst);
    }

    fn require_ui(&self) -> Result<()> {
        if self.inner.ui_attached.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err(ServiceError::InvalidInput(
                "no UI in this process: use the in-app MCP server (Settings → MCP) to control the app".into(),
            ))
        }
    }

    /// Shows a view in the app. `route` is one of [`ROUTES`].
    pub async fn ui_navigate(&self, route: String) -> Result<()> {
        self.require_ui()?;
        if !ROUTES.contains(&route.as_str()) {
            return Err(ServiceError::InvalidInput(format!(
                "unknown route {route:?}; one of {}",
                ROUTES.join(", ")
            )));
        }
        self.emit(Event::UiNavigate { route, asset_id: None });
        Ok(())
    }

    /// Opens a library asset in its editor (token/cut-out cutter, map paginator).
    /// Returns the route that was opened.
    pub async fn ui_open_asset(&self, id: i64) -> Result<String> {
        self.require_ui()?;
        let asset = self.library_get(id).await?;
        let route = editor_route(&asset.kind).ok_or_else(|| {
            ServiceError::InvalidInput(format!("assets of kind {:?} have no editor", asset.kind))
        })?;
        self.emit(Event::UiNavigate { route: route.to_string(), asset_id: Some(id) });
        Ok(route.to_string())
    }
}
