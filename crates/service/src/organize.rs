//! Tags, campaigns and sessions. Specs: docs/features/tags.md, campaigns-sessions.md.

use sleuth_store::{Campaign, RenameOutcome, Session, TagCount};

use crate::{Result, Service};

impl Service {
    pub async fn tags_list(&self) -> Result<Vec<TagCount>> {
        self.blocking(|s| Ok(s.list_tags()?)).await
    }

    /// Renames a tag, merging into an existing tag with the same name if any.
    pub async fn tags_rename(&self, id: i64, name: String) -> Result<RenameOutcome> {
        self.blocking(move |s| Ok(s.rename_tag(id, &name)?)).await
    }

    pub async fn tags_merge(&self, src: i64, dst: i64) -> Result<()> {
        self.blocking(move |s| Ok(s.merge_tags(src, dst)?)).await
    }

    pub async fn tags_delete(&self, id: i64) -> Result<()> {
        self.blocking(move |s| Ok(s.delete_tag(id)?)).await
    }

    pub async fn campaigns_list(&self) -> Result<Vec<Campaign>> {
        self.blocking(|s| Ok(s.list_campaigns()?)).await
    }

    pub async fn campaigns_create(&self, name: String) -> Result<Campaign> {
        self.blocking(move |s| Ok(s.create_campaign(&name)?)).await
    }

    pub async fn campaigns_rename(&self, id: String, name: String) -> Result<()> {
        self.blocking(move |s| Ok(s.rename_campaign(&id, &name)?)).await
    }

    /// Deletes a campaign with its sessions and links; assets are kept.
    pub async fn campaigns_delete(&self, id: String) -> Result<()> {
        self.blocking(move |s| Ok(s.delete_campaign(&id)?)).await
    }

    pub async fn sessions_list(&self, campaign_id: Option<String>) -> Result<Vec<Session>> {
        self.blocking(move |s| Ok(s.list_sessions(campaign_id.as_deref())?)).await
    }

    pub async fn sessions_create(
        &self,
        campaign_id: String,
        name: String,
        notes: String,
    ) -> Result<Session> {
        self.blocking(move |s| Ok(s.create_session(&campaign_id, &name, &notes)?)).await
    }

    pub async fn sessions_update(
        &self,
        id: String,
        name: Option<String>,
        notes: Option<String>,
    ) -> Result<()> {
        self.blocking(move |s| Ok(s.update_session(&id, name.as_deref(), notes.as_deref())?)).await
    }

    pub async fn sessions_delete(&self, id: String) -> Result<()> {
        self.blocking(move |s| Ok(s.delete_session(&id)?)).await
    }

    pub async fn sessions_set_assets(&self, id: String, asset_ids: Vec<i64>) -> Result<()> {
        self.blocking(move |s| Ok(s.set_session_assets(&id, &asset_ids)?)).await
    }
}

#[cfg(test)]
mod tests {
    use crate::temp_service;

    #[tokio::test]
    async fn campaign_session_roundtrip() {
        let (_d, svc) = temp_service();
        let c = svc.campaigns_create("Phandelver".into()).await.unwrap();
        let s =
            svc.sessions_create(c.id.clone(), "Goblin Arrows".into(), String::new()).await.unwrap();
        svc.sessions_update(s.id.clone(), None, Some("ambush at the road".into())).await.unwrap();
        let list = svc.sessions_list(Some(c.id.clone())).await.unwrap();
        assert_eq!(list[0].notes, "ambush at the road");
        svc.campaigns_delete(c.id).await.unwrap();
        assert!(svc.sessions_list(None).await.unwrap().is_empty());
    }
}
