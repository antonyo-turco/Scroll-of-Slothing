//! Session planner: everything the next session needs, gathered by reference
//! from the other tools (no copies). Spec: docs/features/session-planner.md.
//!
//! Pure data and rules; the service resolves the references, stores the
//! planner and prints it.

use serde::{Deserialize, Serialize};

/// What a planned item points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefKind {
    /// A library asset (token, cut-out, map …), by its numeric id.
    Asset,
    /// A saved combat encounter.
    Encounter,
    Soundscape,
    Playlist,
    /// A music track.
    Track,
    /// A saved card deck (Cards), by its numeric id.
    Deck,
}

/// Where an item shows in the planner, in this order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Section {
    Music,
    Encounters,
    Tokens,
    Maps,
    Handouts,
}

/// Section of a reference; assets go by their kind (`token`, `cutout`, `map`, other).
pub fn section(kind: RefKind, asset_kind: Option<&str>) -> Section {
    match kind {
        RefKind::Soundscape | RefKind::Playlist | RefKind::Track => Section::Music,
        RefKind::Encounter => Section::Encounters,
        RefKind::Deck => Section::Handouts,
        RefKind::Asset => match asset_kind {
            Some("token" | "cutout") => Section::Tokens,
            Some("map") => Section::Maps,
            _ => Section::Handouts,
        },
    }
}

/// Assets that print on the token sheet.
pub fn prints_on_sheet(asset_kind: &str) -> bool {
    matches!(asset_kind, "token" | "cutout")
}

fn one() -> u32 {
    1
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanItem {
    pub id: String,
    pub kind: RefKind,
    /// The referenced item's id (numbers as text for assets and decks).
    pub ref_id: String,
    /// Copies to print (tokens and cut-outs), 1–99.
    #[serde(default = "one")]
    pub copies: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CheckItem {
    pub id: String,
    pub text: String,
    #[serde(default)]
    pub done: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionPlan {
    pub id: String,
    pub name: String,
    /// Free text, usually `YYYY-MM-DD`.
    #[serde(default)]
    pub date: Option<String>,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub checklist: Vec<CheckItem>,
    #[serde(default)]
    pub items: Vec<PlanItem>,
    /// Print the Token Sheet's current contents along with the planned tokens.
    #[serde(default)]
    pub include_token_sheet: bool,
    pub created: String,
    pub updated: String,
}

impl SessionPlan {
    pub fn new(id: String, name: String, now: String) -> Self {
        SessionPlan {
            id,
            name,
            date: None,
            notes: String::new(),
            checklist: vec![],
            items: vec![],
            include_token_sheet: false,
            created: now.clone(),
            updated: now,
        }
    }

    pub fn find(&self, kind: RefKind, ref_id: &str) -> Option<&PlanItem> {
        self.items.iter().find(|i| i.kind == kind && i.ref_id == ref_id)
    }

    /// Adds a reference once: a second add of the same item changes nothing
    /// and returns the existing item's id.
    pub fn add(&mut self, id: String, kind: RefKind, ref_id: String, copies: u32) -> String {
        if let Some(i) = self.find(kind, &ref_id) {
            return i.id.clone();
        }
        self.items.push(PlanItem { id: id.clone(), kind, ref_id, copies: copies.clamp(1, 99) });
        id
    }

    pub fn remove(&mut self, item_id: &str) -> bool {
        let n = self.items.len();
        self.items.retain(|i| i.id != item_id);
        self.items.len() != n
    }

    /// Drops references whose target is gone (`exists` says which still are).
    pub fn prune(&mut self, exists: impl Fn(&PlanItem) -> bool) -> usize {
        let n = self.items.len();
        self.items.retain(|i| exists(i));
        n - self.items.len()
    }

    /// Checklist items done / total.
    pub fn progress(&self) -> (usize, usize) {
        (self.checklist.iter().filter(|c| c.done).count(), self.checklist.len())
    }
}

/// All session plans and which one is "the next session".
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Planner {
    #[serde(default)]
    pub sessions: Vec<SessionPlan>,
    /// The session that starred items join automatically.
    #[serde(default)]
    pub current: Option<String>,
}

impl Planner {
    pub fn get(&self, id: &str) -> Option<&SessionPlan> {
        self.sessions.iter().find(|s| s.id == id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut SessionPlan> {
        self.sessions.iter_mut().find(|s| s.id == id)
    }

    /// Adds a session; the first one becomes current.
    pub fn push(&mut self, plan: SessionPlan) {
        if self.current.is_none() {
            self.current = Some(plan.id.clone());
        }
        self.sessions.push(plan);
    }

    /// Removes a session. When it was current, the newest remaining one
    /// (last in the list) becomes current.
    pub fn remove(&mut self, id: &str) -> bool {
        let n = self.sessions.len();
        self.sessions.retain(|s| s.id != id);
        if self.current.as_deref() == Some(id) {
            self.current = self.sessions.last().map(|s| s.id.clone());
        }
        self.sessions.len() != n
    }

    pub fn set_current(&mut self, id: &str) -> bool {
        if self.get(id).is_none() {
            return false;
        }
        self.current = Some(id.to_string());
        true
    }

    pub fn is_current(&self, id: &str) -> bool {
        self.current.as_deref() == Some(id)
    }
}

/// Starred references that join `plan` because it is the current session:
/// those not already planned, in the given order, without duplicates.
pub fn starred_extras(plan: &SessionPlan, starred: &[(RefKind, String)]) -> Vec<(RefKind, String)> {
    let mut out: Vec<(RefKind, String)> = Vec::new();
    for (k, r) in starred {
        if plan.find(*k, r).is_none() && !out.iter().any(|(k2, r2)| k2 == k && r2 == r) {
            out.push((*k, r.clone()));
        }
    }
    out
}

/// A safe file name for a printed part: letters, digits, spaces, `-` and `_`
/// kept, the rest dropped, trimmed, never empty.
pub fn file_stem(name: &str) -> String {
    let s: String = name
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_'))
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let s: String = s.chars().take(60).collect();
    if s.is_empty() { "untitled".into() } else { s }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan() -> SessionPlan {
        SessionPlan::new("s1".into(), "Session 1".into(), "now".into())
    }

    #[test]
    fn adding_twice_keeps_one_reference() {
        let mut p = plan();
        let a = p.add("i1".into(), RefKind::Asset, "7".into(), 3);
        let b = p.add("i2".into(), RefKind::Asset, "7".into(), 1);
        assert_eq!((a.as_str(), b.as_str()), ("i1", "i1"));
        assert_eq!(p.items.len(), 1);
        assert_eq!(p.items[0].copies, 3);
        // same id, other kind: a different reference
        p.add("i3".into(), RefKind::Deck, "7".into(), 0);
        assert_eq!(p.items.len(), 2);
        assert_eq!(p.items[1].copies, 1, "copies clamp to 1–99");
        assert!(p.remove("i1"));
        assert!(!p.remove("i1"));
    }

    #[test]
    fn first_session_is_current_and_removal_moves_it() {
        let mut pl = Planner::default();
        pl.push(SessionPlan::new("a".into(), "A".into(), "t".into()));
        pl.push(SessionPlan::new("b".into(), "B".into(), "t".into()));
        assert!(pl.is_current("a"));
        assert!(pl.set_current("b"));
        assert!(!pl.set_current("zzz"));
        assert!(pl.remove("b"));
        assert_eq!(pl.current.as_deref(), Some("a"));
        assert!(pl.remove("a"));
        assert_eq!(pl.current, None);
    }

    #[test]
    fn starred_items_join_once_and_skip_planned_ones() {
        let mut p = plan();
        p.add("i1".into(), RefKind::Encounter, "e1".into(), 1);
        let starred = vec![
            (RefKind::Encounter, "e1".to_string()),
            (RefKind::Asset, "4".to_string()),
            (RefKind::Asset, "4".to_string()),
            (RefKind::Track, "t".to_string()),
        ];
        assert_eq!(
            starred_extras(&p, &starred),
            vec![(RefKind::Asset, "4".to_string()), (RefKind::Track, "t".to_string())]
        );
    }

    #[test]
    fn sections_follow_the_kind() {
        assert_eq!(section(RefKind::Track, None), Section::Music);
        assert_eq!(section(RefKind::Asset, Some("cutout")), Section::Tokens);
        assert_eq!(section(RefKind::Asset, Some("map")), Section::Maps);
        assert_eq!(section(RefKind::Asset, Some("handout")), Section::Handouts);
        assert_eq!(section(RefKind::Deck, None), Section::Handouts);
        assert!(Section::Music < Section::Handouts);
    }

    #[test]
    fn prune_and_progress() {
        let mut p = plan();
        p.add("i1".into(), RefKind::Asset, "1".into(), 1);
        p.add("i2".into(), RefKind::Asset, "2".into(), 1);
        assert_eq!(p.prune(|i| i.ref_id == "2"), 1);
        assert_eq!(p.items.len(), 1);
        p.checklist = vec![
            CheckItem { id: "c1".into(), text: "Print maps".into(), done: true },
            CheckItem { id: "c2".into(), text: "Recap".into(), done: false },
        ];
        assert_eq!(p.progress(), (1, 2));
    }

    #[test]
    fn file_stems_are_safe() {
        assert_eq!(file_stem("Goblin Cave: Level 1/2"), "Goblin Cave Level 12");
        assert_eq!(file_stem("  ***  "), "untitled");
    }
}
