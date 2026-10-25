//! 5etools combat links (`dmscreen.html#combat=…` / `#encounter=…`): JSON,
//! raw-deflated, base64url without padding (5etools `UrlUtil.Compress`), and
//! Bestiary encounters rolled into tracker rows.

use std::io::{Read, Write};

use base64::Engine;
use base64::engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD};
use serde_json::Value;

use super::{Column, NewId, Rng, Row, Tracker, creature};

pub const PREFIX_COMBAT: &str = "combat=";
pub const PREFIX_ENCOUNTER: &str = "encounter=";

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkKind {
    Combat,
    Encounter,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum LinkError {
    #[error("not a 5etools combat or encounter link")]
    NotALink,
    #[error("the link data is damaged: {0}")]
    Damaged(String),
}

pub fn compress(v: &Value) -> String {
    let mut enc = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::best());
    enc.write_all(v.to_string().as_bytes()).expect("writing to a Vec cannot fail");
    URL_SAFE_NO_PAD.encode(enc.finish().expect("writing to a Vec cannot fail"))
}

pub fn decompress(s: &str) -> Result<Value, LinkError> {
    let s = percent_decode(s.trim());
    let bytes = URL_SAFE_NO_PAD
        .decode(s.trim_end_matches('='))
        .or_else(|_| URL_SAFE.decode(&s))
        .map_err(|e| LinkError::Damaged(e.to_string()))?;
    let mut out = String::new();
    flate2::read::DeflateDecoder::new(&bytes[..])
        .read_to_string(&mut out)
        .map_err(|e| LinkError::Damaged(e.to_string()))?;
    serde_json::from_str(&out).map_err(|e| LinkError::Damaged(e.to_string()))
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let Ok(x) = u8::from_str_radix(&s[i + 1..i + 3], 16)
        {
            out.push(x);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Reads a full URL, a `#hash`, or a hash without `#`. A bare payload (no
/// prefix) is read as a combat.
pub fn parse(text: &str) -> Result<(LinkKind, Value), LinkError> {
    let t = text.trim();
    if t.is_empty() {
        return Err(LinkError::NotALink);
    }
    let hash = t.split_once('#').map_or(t, |(_, h)| h);
    if let Some(p) = hash.strip_prefix(PREFIX_COMBAT) {
        return Ok((LinkKind::Combat, decompress(p)?));
    }
    if let Some(p) = hash.strip_prefix(PREFIX_ENCOUNTER) {
        return Ok((LinkKind::Encounter, decompress(p)?));
    }
    if t.contains('#') || t.contains("://") || t.contains('=') {
        return Err(LinkError::NotALink);
    }
    Ok((LinkKind::Combat, decompress(t)?))
}

/// `<base>/dmscreen.html#combat=<payload>`.
pub fn combat_url(base_url: &str, tracker: &Tracker) -> String {
    format!(
        "{}/dmscreen.html#{PREFIX_COMBAT}{}",
        base_url.trim_end_matches('/'),
        compress(&tracker.to_serial())
    )
}

/// One creature line of an exported Bestiary encounter.
#[derive(Debug, Clone, PartialEq)]
pub struct EncounterItem {
    pub hash: String,
    pub count: u32,
    /// Set for scaled creatures (not applied yet, see the feature doc).
    pub custom_hash_id: Option<String>,
}

/// A 5etools exported Bestiary encounter (what "Save to File" and the
/// `#encounter=` link carry) for these creature lines.
pub fn exported_encounter(name: &str, items: &[EncounterItem]) -> Value {
    let mut sources: Vec<String> = items
        .iter()
        .filter_map(|i| i.hash.rsplit_once('_').map(|(_, s)| s.to_uppercase()))
        .collect();
    sources.sort();
    sources.dedup();
    let items: Vec<Value> = items
        .iter()
        .map(|i| {
            let mut o = serde_json::Map::new();
            o.insert("h".into(), Value::String(i.hash.clone()));
            if i.count > 1 {
                o.insert("c".into(), Value::from(i.count));
            }
            if let Some(c) = &i.custom_hash_id {
                o.insert("customHashId".into(), Value::String(c.clone()));
            }
            Value::Object(o)
        })
        .collect();
    serde_json::json!({"name": name, "items": items, "sources": sources})
}

pub fn encounter_items(enc: &Value) -> Vec<EncounterItem> {
    enc.get("items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|it| {
            Some(EncounterItem {
                hash: it.get("h")?.as_str()?.to_lowercase(),
                count: it.get("c").and_then(Value::as_u64).unwrap_or(1).clamp(1, 500) as u32,
                custom_hash_id: it.get("customHashId").and_then(Value::as_str).map(Into::into),
            })
        })
        .collect()
}

/// What an encounter import produced.
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize)]
pub struct EncounterImport {
    pub rows_added: usize,
    /// Creature hashes the bestiary cache does not have (reported, not dropped silently).
    pub missing: Vec<String>,
    /// Scaled creatures imported at their listed stats.
    pub unscaled: Vec<String>,
}

/// Rolls an exported encounter into the tracker with its import settings
/// (5etools `InitiativeTrackerEncounterConverter` + `pDoLoadEncounter`):
/// players from the encounter builder's party, then each creature `count`
/// times; replaces the rows unless "add to existing" is on.
pub fn import_encounter(
    t: &mut Tracker,
    enc: &Value,
    lookup: &dyn Fn(&str) -> Option<Value>,
    base_url: &str,
    rng: Rng,
    new_id: NewId,
) -> EncounterImport {
    let mut out = EncounterImport::default();
    let mut rows: Vec<Row> = vec![];

    if t.import_is_add_players {
        let party = |id: &str| enc.get("statePartyComps")?.get(id)?.get("state").cloned();
        match enc.get("activePartyId").and_then(Value::as_str) {
            Some("customAdvanced") => {
                if let Some(st) = party("customAdvanced") {
                    import_advanced_players(t, &st, &mut rows, new_id);
                }
            }
            Some("custom") => {
                if let Some(st) = party("custom") {
                    for g in st.get("playersSimple").and_then(Value::as_array).into_iter().flatten()
                    {
                        for _ in 0..g.get("count").and_then(Value::as_u64).unwrap_or(1).min(50) {
                            rows.push(t.new_player(new_id));
                        }
                    }
                }
            }
            _ => {}
        }
        if rows.is_empty() {
            rows.extend(t.rows_default_party.iter().map(|r| Row {
                id: new_id(),
                is_active: false,
                ..r.clone()
            }));
        }
    }

    for item in encounter_items(enc) {
        let Some(mon) = lookup(&item.hash) else {
            out.missing.push(item.hash);
            continue;
        };
        if item.custom_hash_id.is_some() {
            out.unscaled.push(item.hash.clone());
        }
        let bonus = creature::initiative_bonus(&mon).unwrap_or(0);
        let group_init = (t.import_is_roll_groups && t.is_roll_init)
            .then(|| creature::roll_initiative(bonus, rng));
        let group_hp =
            t.import_is_roll_groups.then(|| creature::hp_for(&mon, t.is_roll_hp, rng)).flatten();
        for _ in 0..item.count {
            let hp = if t.import_is_roll_groups {
                group_hp
            } else {
                creature::hp_for(&mon, t.is_roll_hp, rng)
            };
            let init = if !t.is_roll_init {
                None
            } else if t.import_is_roll_groups {
                group_init
            } else {
                Some(creature::roll_initiative(bonus, rng))
            };
            let all: Vec<Row> = if t.import_is_append {
                t.rows.iter().chain(&rows).cloned().collect()
            } else {
                rows.clone()
            };
            let row = t.new_creature(&mon, &all, hp, init, None, base_url, new_id);
            rows.push(row);
        }
    }

    out.rows_added = rows.len();
    if t.import_is_append {
        t.rows.extend(rows);
    } else {
        t.rows = rows;
        let d = Tracker::default();
        t.round = d.round;
        t.sort = d.sort;
        t.dir = d.dir;
    }
    t.sort_rows();
    out
}

/// Encounter builder "advanced" party: named players with extra columns; a
/// column called `HP` fills `current/max`, the others become stat columns.
fn import_advanced_players(t: &mut Tracker, st: &Value, rows: &mut Vec<Row>, new_id: NewId) {
    let cols: Vec<String> = st
        .get("colsExtraAdvanced")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|c| c.get("name").and_then(Value::as_str).unwrap_or("").to_lowercase())
        .collect();
    let kinds = creature::column_kinds();
    let mut col_ids: Vec<Option<String>> = vec![];
    for name in &cols {
        if name == "hp" {
            col_ids.push(None);
            continue;
        }
        let k = kinds
            .iter()
            .find(|k| !k.abbreviation.is_empty() && k.abbreviation.to_lowercase() == name.trim());
        let col = Column {
            id: new_id(),
            is_editable: true,
            is_player_visible: 2,
            populate_with: k.map(|k| k.populate_with.clone()).unwrap_or_default(),
            abbreviation: name.clone(),
        };
        col_ids.push(Some(col.id.clone()));
        t.stats_cols.push(col);
    }
    if !cols.is_empty() {
        t.is_stats_add_columns = true;
    }
    for p in st.get("playersAdvanced").and_then(Value::as_array).into_iter().flatten() {
        let mut row = t.new_player(new_id);
        row.name = p.get("name").and_then(Value::as_str).unwrap_or("").into();
        let extras = p.get("extras").and_then(Value::as_array).cloned().unwrap_or_default();
        for (i, ex) in extras.iter().enumerate() {
            let val = ex.get("value").and_then(Value::as_str).unwrap_or("").to_string();
            match col_ids.get(i) {
                Some(None) => {
                    let mut parts = val.split('/').map(|s| s.trim().parse::<i64>().ok());
                    let cur = parts.next().flatten();
                    let max = parts.next().flatten();
                    row.hp_current = cur;
                    row.hp_max = max.or(cur);
                }
                Some(Some(id)) => {
                    if let Some(c) = row.cells.iter_mut().find(|c| &c.id == id) {
                        c.entity.insert("value".into(), Value::String(val));
                    }
                }
                None => {}
            }
        }
        rows.push(row);
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn ids() -> impl FnMut() -> String {
        let mut n = 0;
        move || {
            n += 1;
            format!("id{n}")
        }
    }

    #[test]
    fn compress_round_trips_and_reads_5etools_payloads() {
        let v = json!({"r": [{"n": "Goblin", "i": 12}], "n": 3});
        let s = compress(&v);
        assert!(!s.contains('=') && !s.contains('+') && !s.contains('/'));
        assert_eq!(decompress(&s).unwrap(), v);
        // produced by 5etools UrlUtil.Compress.pCompress({a: 1})
        assert_eq!(decompress("q1ZKVLIyrAUA").unwrap(), json!({"a": 1}));
        let url = format!("http://localhost:8080/dmscreen.html#combat={s}");
        assert_eq!(parse(&url).unwrap(), (LinkKind::Combat, v.clone()));
        assert_eq!(parse(&format!("#encounter={s}")).unwrap().0, LinkKind::Encounter);
        assert_eq!(parse(&s).unwrap().1, v);
        assert_eq!(parse("http://x/bestiary.html#goblin_mm"), Err(LinkError::NotALink));
        assert!(matches!(parse("combat=@@@"), Err(LinkError::Damaged(_))));
    }

    #[test]
    fn combat_url_restores_the_tracker() {
        let mut t = Tracker { round: 4, notes: "ambush".into(), ..Tracker::default() };
        t.rows.push(Row {
            id: "a".into(),
            name: "Goblin".into(),
            source: Some("MM".into()),
            ordinal: 1,
            ..Row::default()
        });
        let url = combat_url("http://localhost:8080/", &t);
        assert!(url.starts_with("http://localhost:8080/dmscreen.html#combat="));
        let (_, v) = parse(&url).unwrap();
        assert_eq!(Tracker::from_serial(&v, &mut ids()), t);
    }

    #[test]
    fn encounter_rolls_players_and_creatures() {
        let goblin = json!({"name": "Goblin", "source": "MM", "hp": {"average": 7, "formula": "2d6"}, "dex": 14});
        let enc = json!({
            "items": [{"h": "goblin_mm", "c": 3}, {"h": "nope_xyz", "c": 1}, {"h": "goblin_mm", "c": 1, "customHashId": "x"}],
            "sources": ["MM"],
            "activePartyId": "customAdvanced",
            "statePartyComps": {"customAdvanced": {"state": {
                "colsExtraAdvanced": [{"name": "HP"}, {"name": "AC"}],
                "playersAdvanced": [{"name": "Vex", "extras": [{"value": "30/42"}, {"value": "17"}]}]
            }}}
        });
        let lookup = |h: &str| (h == "goblin_mm").then(|| goblin.clone());
        let mut t = Tracker::default();
        let mut rng = |_| 9; // d20 → 10
        let r = import_encounter(&mut t, &enc, &lookup, "http://h", &mut rng, &mut ids());
        assert_eq!(r.missing, vec!["nope_xyz".to_string()]);
        assert_eq!(r.unscaled, vec!["goblin_mm".to_string()]);
        assert_eq!(r.rows_added, 5);
        let vex = t.rows.iter().find(|r| r.name == "Vex").unwrap();
        assert_eq!((vex.hp_current, vex.hp_max), (Some(30), Some(42)));
        assert_eq!(t.stats_cols.len(), 1);
        assert_eq!(t.stats_cols[0].populate_with, "armorClass");
        assert_eq!(vex.cells[0].entity["value"], json!("17"));
        let gobs: Vec<_> = t.rows.iter().filter(|r| r.name == "Goblin").collect();
        assert_eq!(gobs.len(), 4);
        assert!(gobs.iter().all(|g| g.initiative == Some(12) && g.hp_max == Some(7)));
        let mut ords: Vec<i64> = gobs.iter().map(|g| g.ordinal).collect();
        ords.sort();
        assert_eq!(ords, [1, 2, 3, 4]);
        assert_eq!(t.rows[0].name, "Goblin", "sorted by initiative, desc; Vex has none");
    }
}
