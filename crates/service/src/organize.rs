//! Tags. Spec: docs/features/tags.md. (Campaigns and sessions were removed from the
//! app at the user's request; their tables are kept, unused.)

use sleuth_store::{RenameOutcome, TagCount};

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
}
