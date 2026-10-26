//! Card printer service. Spec: docs/features/cards.md.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sleuth_core::cards::{Card, DeckLayout, Flip, layout, parse};
use sleuth_core::units::PageFormat;
use sleuth_pdf::PdfError;
use sleuth_store::{Deck, DeckSummary};

use crate::token_sheet::check_sheet_format;
use crate::{JobId, Result, Service, ServiceError};

/// Verbatim Python `TEMPLATES` and `REFERENCE_TEXT` (assets/cards/templates.json).
const TEMPLATES_JSON: &str = include_str!("../../../assets/cards/templates.json");

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CardOptions {
    pub format: PageFormat,
    #[serde(default)]
    pub double_sided: bool,
    #[serde(default)]
    pub flip: Flip,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CardSummary {
    /// Cards after expanding copies.
    pub cards: usize,
    pub pages: usize,
    pub overflow: Vec<String>,
    pub backs: bool,
}

impl From<&DeckLayout> for CardSummary {
    fn from(l: &DeckLayout) -> Self {
        CardSummary {
            cards: l.cards,
            pages: l.pages.len(),
            overflow: l.overflow.clone(),
            backs: l.backs,
        }
    }
}

impl Service {
    /// `{templates: {Spell, Feat, Item, Ability}, reference}`.
    pub fn cards_templates(&self) -> Value {
        serde_json::from_str(TEMPLATES_JSON).expect("bundled templates are valid JSON")
    }

    pub async fn cards_parse(&self, markdown: String) -> Result<Vec<Card>> {
        self.blocking(move |_| Ok(parse(&markdown))).await
    }

    /// Full page layout (drawing ops) for the exact preview.
    pub async fn cards_layout(&self, markdown: String, opts: CardOptions) -> Result<DeckLayout> {
        check_sheet_format(opts.format)?;
        self.blocking(move |_| {
            Ok(layout(&parse(&markdown), opts.format, opts.double_sided, opts.flip))
        })
        .await
    }

    /// Counts, page count and overflow warnings (no drawing ops).
    pub async fn cards_validate(&self, markdown: String, opts: CardOptions) -> Result<CardSummary> {
        let l = self.cards_layout(markdown, opts).await?;
        Ok(CardSummary::from(&l))
    }

    /// Exports the deck as a PDF (job result: `{path, pages, cards, overflow}`).
    pub fn cards_export_pdf(&self, markdown: String, opts: CardOptions, path: PathBuf) -> JobId {
        self.spawn_job("card export", move |ctx| {
            check_sheet_format(opts.format)?;
            let cards = parse(&markdown);
            if cards.is_empty() {
                return Err(ServiceError::InvalidInput("the deck has no cards".into()));
            }
            ctx.progress(0.05, "laying out cards");
            let l = layout(&cards, opts.format, opts.double_sided, opts.flip);
            let bytes = sleuth_pdf::cards::render(&l, &mut |p| {
                ctx.progress(0.1 + 0.85 * p, "rendering pages");
                !ctx.is_cancelled()
            })
            .map_err(|e| match e {
                PdfError::Cancelled => ServiceError::Cancelled,
                other => ServiceError::Internal(other.to_string()),
            })?;
            std::fs::write(&path, bytes)?;
            Ok(serde_json::json!({ "path": path, "pages": l.pages.len(), "cards": l.cards, "overflow": l.overflow }))
        })
    }

    pub async fn decks_list(&self) -> Result<Vec<DeckSummary>> {
        self.blocking(|s| Ok(s.list_decks()?)).await
    }

    pub async fn decks_get(&self, id: i64) -> Result<Deck> {
        self.blocking(move |s| Ok(s.get_deck(id)?)).await
    }

    pub async fn decks_save(
        &self,
        id: Option<i64>,
        name: String,
        markdown: String,
        opts: Option<CardOptions>,
    ) -> Result<Deck> {
        let settings =
            serde_json::to_string(&opts).map_err(|e| ServiceError::Internal(e.to_string()))?;
        self.blocking(move |s| Ok(s.save_deck(id, &name, &markdown, &settings)?)).await
    }

    pub async fn decks_delete(&self, id: i64) -> Result<()> {
        self.blocking(move |s| Ok(s.delete_deck(id)?)).await
    }

    /// Reads a `.md`/`.txt` deck file (UTF-8).
    pub async fn deck_read_file(&self, path: PathBuf) -> Result<String> {
        self.blocking(move |_| {
            let bytes = std::fs::read(&path)?;
            String::from_utf8(bytes)
                .map_err(|_| ServiceError::InvalidInput("the file is not UTF-8 text".into()))
        })
        .await
    }

    pub async fn deck_write_file(&self, markdown: String, path: PathBuf) -> Result<PathBuf> {
        self.blocking(move |_| {
            std::fs::write(&path, markdown)?;
            Ok(path)
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::{JobState, temp_service};

    const DECK: &str = "# Fireball\n**Type:** Spell\n**Copies:** 2\n\nBoom.\n---\n# Lucky\n**Type:** Feat\n\nLuck.";

    #[tokio::test]
    async fn templates_are_the_python_ones() {
        let (_d, svc) = temp_service();
        let t = svc.cards_templates();
        assert!(
            t["templates"]["Spell"].as_str().unwrap().starts_with("# Spell Name\n**Type:** Spell")
        );
        assert!(
            t["reference"]
                .as_str()
                .unwrap()
                .starts_with("Card Printer — Markdown Format Reference")
        );
        let parsed =
            svc.cards_parse(t["templates"]["Item"].as_str().unwrap().into()).await.unwrap();
        assert_eq!(parsed[0].kind, "Item");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn validate_export_and_decks() {
        let (d, svc) = temp_service();
        let opts = CardOptions { format: PageFormat::A4, double_sided: false, flip: Flip::Long };
        let s = svc.cards_validate(DECK.into(), opts).await.unwrap();
        assert_eq!((s.cards, s.pages, s.backs), (3, 1, false));

        let out = d.path().join("deck.pdf");
        let job = svc.cards_export_pdf(DECK.into(), opts, out.clone());
        let mut st = svc.job_status(&job).unwrap();
        for _ in 0..1000 {
            st = svc.job_status(&job).unwrap();
            if st.state != JobState::Running {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(st.state, JobState::Done, "{:?}", st.error);
        assert!(std::fs::read(&out).unwrap().starts_with(b"%PDF"));

        let deck = svc.decks_save(None, "Starter".into(), DECK.into(), Some(opts)).await.unwrap();
        assert_eq!(svc.decks_list().await.unwrap()[0].name, "Starter");
        let back: CardOptions =
            serde_json::from_str(&svc.decks_get(deck.id).await.unwrap().settings_json).unwrap();
        assert_eq!(back, opts);

        let md = d.path().join("deck.md");
        svc.deck_write_file(DECK.into(), md.clone()).await.unwrap();
        assert_eq!(svc.deck_read_file(md).await.unwrap(), DECK);
        svc.decks_delete(deck.id).await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn empty_deck_export_fails_cleanly() {
        let (d, svc) = temp_service();
        let opts =
            CardOptions { format: PageFormat::Letter, double_sided: false, flip: Flip::Long };
        let job = svc.cards_export_pdf("   \n".into(), opts, d.path().join("x.pdf"));
        for _ in 0..1000 {
            if svc.job_status(&job).unwrap().state != JobState::Running {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(svc.job_status(&job).unwrap().state, JobState::Failed);
    }
}
