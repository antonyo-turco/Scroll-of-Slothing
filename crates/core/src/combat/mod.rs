//! Combat (initiative) tracker: a port of the 5etools DM Screen Initiative
//! Tracker (`js/dmscreen/panels/initiativetracker/`). The state converts to and
//! from the 5etools serial form, so combats round-trip through 5etools.
//! See `docs/features/combat-tracker.md`.

pub mod creature;
pub mod link;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

/// Source of random numbers: `rng(n)` returns `0..n`.
pub type Rng<'a> = &'a mut dyn FnMut(u32) -> u32;
/// Source of fresh row/condition ids.
pub type NewId<'a> = &'a mut dyn FnMut() -> String;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Sort {
    #[serde(rename = "ALPHA")]
    Alpha,
    #[default]
    #[serde(rename = "NUMBER")]
    Number,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Dir {
    #[serde(rename = "ASC")]
    Asc,
    #[default]
    #[serde(rename = "DESC")]
    Desc,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Condition {
    pub id: String,
    pub name: String,
    pub color: String,
    /// Turns left; None is unlimited.
    pub turns: Option<i64>,
}

/// One stat cell: the column id plus its fields (`value`, or `current`/`max`,
/// or `tokenUrl`/`imageHref`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cell {
    pub id: String,
    #[serde(flatten)]
    pub entity: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Row {
    pub id: String,
    pub name: String,
    pub display_name: Option<String>,
    pub custom_name: Option<String>,
    /// Bestiary source; None for players.
    pub source: Option<String>,
    pub scaled_cr: Option<f64>,
    pub scaled_summon_spell_level: Option<i64>,
    pub scaled_summon_class_level: Option<i64>,
    pub hp_current: Option<i64>,
    pub hp_max: Option<i64>,
    pub initiative: Option<i64>,
    pub is_active: bool,
    pub conditions: Vec<Condition>,
    pub notes: String,
    pub is_player_visible: bool,
    pub ordinal: i64,
    pub cells: Vec<Cell>,
}

impl Row {
    pub fn is_creature(&self) -> bool {
        self.source.as_deref().is_some_and(|s| !s.is_empty())
    }

    /// 5etools `getSimilarRowEntityHash`: rows with the same key are copies.
    pub fn similar_key(&self) -> String {
        json!([
            self.name,
            self.custom_name,
            self.scaled_cr,
            self.scaled_summon_spell_level,
            self.scaled_summon_class_level,
            self.source
        ])
        .to_string()
    }

    fn label(&self) -> &str {
        self.custom_name.as_deref().filter(|s| !s.is_empty()).unwrap_or(&self.name)
    }
}

/// Who sees a column in the player view (`0` none, `1` all, `2` players only).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Column {
    pub id: String,
    pub is_editable: bool,
    pub is_player_visible: u8,
    pub populate_with: String,
    pub abbreviation: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CustomCondition {
    pub id: String,
    pub name: String,
    pub color: String,
    pub turns: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Tracker {
    pub sort: Sort,
    pub dir: Dir,
    pub is_roll_init: bool,
    pub is_roll_hp: bool,
    pub is_roll_groups: bool,
    pub is_reroll_initiative_each_round: bool,
    pub is_invert_wound_direction: bool,
    pub import_is_roll_groups: bool,
    pub import_is_add_players: bool,
    pub import_is_append: bool,
    pub is_stats_add_columns: bool,
    pub player_init_show_exact_player_hp: bool,
    pub player_init_show_exact_monster_hp: bool,
    pub player_init_hide_new_monster: bool,
    pub player_init_show_ordinals: bool,
    pub is_notes_shown: bool,
    pub stats_cols: Vec<Column>,
    pub conditions_custom: Vec<CustomCondition>,
    pub rows: Vec<Row>,
    pub rows_default_party: Vec<Row>,
    pub round: i64,
    pub notes: String,
    /// Not saved in links (5etools: temporary).
    pub is_locked: bool,
    /// Serial keys this version does not know, kept for round-trips.
    pub extra: Map<String, Value>,
}

impl Default for Tracker {
    /// 5etools `_getDefaultState`.
    fn default() -> Self {
        Tracker {
            sort: Sort::Number,
            dir: Dir::Desc,
            is_roll_init: true,
            is_roll_hp: false,
            is_roll_groups: false,
            is_reroll_initiative_each_round: false,
            is_invert_wound_direction: false,
            import_is_roll_groups: true,
            import_is_add_players: true,
            import_is_append: false,
            is_stats_add_columns: false,
            player_init_show_exact_player_hp: false,
            player_init_show_exact_monster_hp: false,
            player_init_hide_new_monster: true,
            player_init_show_ordinals: false,
            is_notes_shown: false,
            stats_cols: vec![],
            conditions_custom: vec![],
            rows: vec![],
            rows_default_party: vec![],
            round: 1,
            notes: String::new(),
            is_locked: false,
            extra: Map::new(),
        }
    }
}

/// The built-in conditions (5etools `InitiativeTrackerUtil.CONDITIONS`),
/// sorted ignoring punctuation.
pub const CONDITIONS: [(&str, &str); 18] = [
    ("Blinded", "#525252"),
    ("Charmed", "#f01789"),
    ("Concentration", "#009f7a"),
    ("Deafened", "#ababab"),
    ("Drunk", "#ffcc00"),
    ("Exhausted", "#947a47"),
    ("Frightened", "#c9ca18"),
    ("Grappled", "#8784a0"),
    ("Incapacitated", "#3165a0"),
    ("Invisible", "#7ad2d6"),
    ("!!On Fire!!", "#ff6800"),
    ("Paralyzed", "#c00900"),
    ("Petrified", "#a0a0a0"),
    ("Poisoned", "#4dc200"),
    ("Prone", "#5e60a0"),
    ("Restrained", "#d98000"),
    ("Stunned", "#a23bcb"),
    ("Unconscious", "#3a40ad"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WoundLevel {
    Unknown,
    Healthy,
    Hurt,
    Bloodied,
    Defeated,
}

/// 5etools `getWoundLevel`, inverted when HP is tracked as damage.
pub fn wound_level(row: &Row, invert: bool) -> WoundLevel {
    let (Some(cur), Some(max)) = (row.hp_current, row.hp_max) else { return WoundLevel::Unknown };
    if max <= 0 {
        return WoundLevel::Unknown;
    }
    let cur = if invert { max - cur } else { cur };
    let pct = (cur as f64 / max as f64 * 100.0).clamp(0.0, 100.0).round() as i64;
    match pct {
        100 => WoundLevel::Healthy,
        p if p > 50 => WoundLevel::Hurt,
        p if p > 0 => WoundLevel::Bloodied,
        _ => WoundLevel::Defeated,
    }
}

const DIR_FORWARDS: i8 = 1;
const DIR_BACKWARDS: i8 = -1;
const DIR_NEUTRAL: i8 = 0;

impl Tracker {
    // ── sorting ──

    /// 5etools `InitiativeTrackerSort.getSortedRows` (stable, id last).
    pub fn sorted(&self, rows: &[Row]) -> Vec<Row> {
        let mul = |o: std::cmp::Ordering| if self.dir == Dir::Desc { o.reverse() } else { o };
        let name = |a: &Row, b: &Row| mul(a.label().to_lowercase().cmp(&b.label().to_lowercase()));
        let init =
            |a: &Row, b: &Row| mul(a.initiative.unwrap_or(0).cmp(&b.initiative.unwrap_or(0)));
        let ord = |a: &Row, b: &Row| a.ordinal.cmp(&b.ordinal);
        let id = |a: &Row, b: &Row| a.id.cmp(&b.id);
        let mut out = rows.to_vec();
        match self.sort {
            Sort::Alpha => {
                out.sort_by(|a, b| name(a, b).then(ord(a, b)).then(init(a, b)).then(id(a, b)))
            }
            Sort::Number => {
                out.sort_by(|a, b| init(a, b).then(name(a, b)).then(ord(a, b)).then(id(a, b)))
            }
        }
        out
    }

    pub fn sort_rows(&mut self) {
        self.rows = self.sorted(&self.rows);
    }

    /// Sort buttons: picking the active order flips the direction; a new
    /// order starts at name ascending / initiative descending.
    pub fn set_sort(&mut self, sort: Sort) {
        if self.sort == sort {
            self.dir = if self.dir == Dir::Asc { Dir::Desc } else { Dir::Asc };
        } else {
            self.sort = sort;
            self.dir = if sort == Sort::Alpha { Dir::Asc } else { Dir::Desc };
        }
        self.sort_rows();
    }

    // ── rows ──

    /// Ordinal for a new copy: `similar rows + 1`.
    pub fn next_ordinal(rows: &[Row], like: &Row) -> i64 {
        let key = like.similar_key();
        rows.iter().filter(|r| r.similar_key() == key).count() as i64 + 1
    }

    /// True when the row has a similar sibling, so its ordinal is shown.
    pub fn shows_ordinal(&self, row: &Row) -> bool {
        let key = row.similar_key();
        row.is_creature() && self.rows.iter().filter(|r| r.similar_key() == key).count() > 1
    }

    /// Empty cells for every stat column (players, or a creature with `mon`).
    pub fn initial_cells(&self, mon: Option<&Value>, base_url: &str) -> Vec<Cell> {
        self.stats_cols
            .iter()
            .map(|c| {
                let mut e = match creature::initial_cell(&c.populate_with, mon, base_url) {
                    Value::Object(m) => m,
                    _ => Map::new(),
                };
                e.insert("isEditable".into(), json!(c.is_editable));
                Cell { id: c.id.clone(), entity: e }
            })
            .collect()
    }

    /// A blank player row (5etools "Add Player").
    pub fn new_player(&self, new_id: NewId) -> Row {
        Row {
            id: new_id(),
            is_player_visible: true,
            ordinal: 1,
            cells: self.initial_cells(None, ""),
            ..Row::default()
        }
    }

    /// A creature row from its stat block, with HP and initiative already
    /// chosen by the caller (groups share them).
    #[allow(clippy::too_many_arguments)]
    pub fn new_creature(
        &self,
        mon: &Value,
        existing: &[Row],
        hp: Option<i64>,
        initiative: Option<i64>,
        custom_name: Option<String>,
        base_url: &str,
        new_id: NewId,
    ) -> Row {
        let mut row = Row {
            id: new_id(),
            name: mon.get("name").and_then(Value::as_str).unwrap_or("").into(),
            display_name: mon.get("_displayName").and_then(Value::as_str).map(Into::into),
            custom_name: custom_name.filter(|n| !n.trim().is_empty()),
            source: mon.get("source").and_then(Value::as_str).map(Into::into),
            hp_current: if self.is_invert_wound_direction { hp.map(|_| 0) } else { hp },
            hp_max: hp,
            initiative,
            is_player_visible: !self.player_init_hide_new_monster,
            cells: self.initial_cells(Some(mon), base_url),
            ..Row::default()
        };
        row.ordinal = Self::next_ordinal(existing, &row);
        row
    }

    /// Adds `count` copies of a creature (5etools Add Creature): HP average or
    /// rolled, initiative rolled when enabled, shared when rolling groups.
    #[allow(clippy::too_many_arguments)]
    pub fn add_creatures(
        &mut self,
        mon: &Value,
        count: u32,
        roll_hp: bool,
        custom_name: Option<String>,
        base_url: &str,
        rng: Rng,
        new_id: NewId,
    ) -> Vec<String> {
        let bonus = creature::initiative_bonus(mon).unwrap_or(0);
        let group = count > 1 && self.is_roll_groups;
        let group_init = self.is_roll_init.then(|| creature::roll_initiative(bonus, rng));
        let group_hp = creature::hp_for(mon, roll_hp, rng);
        let mut ids = vec![];
        for i in 0..count.max(1) {
            let init = if !self.is_roll_init {
                None
            } else if group || i == 0 {
                group_init
            } else {
                Some(creature::roll_initiative(bonus, rng))
            };
            let hp = if group || i == 0 { group_hp } else { creature::hp_for(mon, roll_hp, rng) };
            let row =
                self.new_creature(mon, &self.rows, hp, init, custom_name.clone(), base_url, new_id);
            ids.push(row.id.clone());
            self.rows.push(row);
        }
        self.is_roll_hp = roll_hp;
        self.sort_rows();
        ids
    }

    /// 5etools "Add Another": a copy keeping the initiative, or with fresh
    /// initiative and HP (`reroll`, needs the stat block).
    pub fn duplicate(
        &mut self,
        id: &str,
        reroll: Option<&Value>,
        rng: Rng,
        new_id: NewId,
    ) -> Option<String> {
        let src = self.rows.iter().find(|r| r.id == id)?.clone();
        let mut row = Row { id: new_id(), is_active: false, conditions: vec![], ..src };
        if let Some(mon) = reroll {
            let bonus = creature::initiative_bonus(mon).unwrap_or(0);
            row.initiative = Some(creature::roll_initiative(bonus, rng));
            let hp = creature::hp_for(mon, true, rng).or(row.hp_max);
            row.hp_max = hp;
            row.hp_current = if self.is_invert_wound_direction { hp.map(|_| 0) } else { hp };
        }
        row.ordinal = Self::next_ordinal(&self.rows, &row);
        let out = row.id.clone();
        self.rows.push(row);
        self.sort_rows();
        Some(out)
    }

    /// Deletes a row, or (`similar`) every row like it.
    pub fn delete(&mut self, id: &str, similar: bool) -> usize {
        let Some(key) = self.rows.iter().find(|r| r.id == id).map(Row::similar_key) else {
            return 0;
        };
        let before = self.rows.len();
        self.rows.retain(|r| if similar { r.similar_key() != key } else { r.id != id });
        before - self.rows.len()
    }

    /// 5etools Reset: the default party, round 1, default sort.
    pub fn reset(&mut self, new_id: NewId) {
        let d = Tracker::default();
        self.rows = self
            .rows_default_party
            .iter()
            .map(|r| Row { id: new_id(), is_active: false, ..r.clone() })
            .collect();
        self.round = d.round;
        self.sort = d.sort;
        self.dir = d.dir;
        self.sort_rows();
    }

    // ── turns ──

    fn column_kind(&self, cell_id: &str) -> Option<&str> {
        self.stats_cols.iter().find(|c| c.id == cell_id).map(|c| c.populate_with.as_str())
    }

    /// Column hooks on a row: `round` for round start, else turn start.
    /// Backwards moves leave cells alone (5etools `_isNonNegativeDirection`).
    fn run_hooks(&mut self, ix: usize, direction: i8, round: bool) {
        if direction == DIR_BACKWARDS {
            return;
        }
        let kinds: Vec<Option<String>> =
            self.rows[ix].cells.iter().map(|c| self.column_kind(&c.id).map(Into::into)).collect();
        for (cell, kind) in self.rows[ix].cells.iter_mut().zip(kinds) {
            let e = &mut cell.entity;
            match (kind.as_deref(), round) {
                (Some("cbAutoLow"), false) | (Some("cbAutoRoundLow"), true) => {
                    e.insert("value".into(), json!(false));
                }
                (Some("cbAutoHigh"), false) | (Some("cbAutoRoundHigh"), true) => {
                    e.insert("value".into(), json!(true));
                }
                (Some("legendaryActions"), true) => {
                    let max = e.get("max").cloned().unwrap_or(Value::Null);
                    e.insert("current".into(), max);
                }
                _ => {}
            }
        }
    }

    /// Activates `rows[ix]` and its similar rows (same initiative when sorting
    /// by number), running turn-start hooks; a row at the top starts a round.
    fn set_active(&mut self, ix: usize, direction: i8, skip_round: bool) -> bool {
        let key = self.rows[ix].similar_key();
        let init = self.rows[ix].initiative;
        let mut round_start = false;
        for j in 0..self.rows.len() {
            let r = &self.rows[j];
            if r.similar_key() != key || (self.sort == Sort::Number && r.initiative != init) {
                continue;
            }
            self.rows[j].is_active = true;
            if !skip_round && j == 0 {
                if direction == DIR_FORWARDS {
                    self.round += 1;
                }
                for k in 0..self.rows.len() {
                    self.run_hooks(k, direction, true);
                }
                round_start = true;
            }
            self.run_hooks(j, direction, false);
        }
        round_start
    }

    /// Makes the first row active when none is (5etools
    /// `doEnsureAtLeastOneRowActive`).
    pub fn ensure_active(&mut self) {
        if self.rows.is_empty() || self.rows.iter().any(|r| r.is_active) {
            return;
        }
        self.sort_rows();
        self.set_active(0, DIR_NEUTRAL, false);
    }

    /// Next turn. `bonus` gives a creature row's initiative bonus for the
    /// reroll-each-round option (None leaves the row's initiative).
    pub fn next_turn(&mut self, rng: Rng, bonus: &dyn Fn(&Row) -> Option<i64>) {
        self.shift(DIR_FORWARDS, rng, bonus);
    }

    pub fn prev_turn(&mut self) {
        self.shift(DIR_BACKWARDS, &mut |_| 0, &|_| None);
    }

    fn shift(&mut self, direction: i8, rng: Rng, bonus: &dyn Fn(&Row) -> Option<i64>) {
        if self.rows.is_empty() {
            return;
        }
        self.sort_rows();
        if !self.rows.iter().any(|r| r.is_active) {
            return self.ensure_active();
        }
        if direction == DIR_BACKWARDS {
            self.rows.reverse();
        }
        if direction == DIR_FORWARDS {
            for r in self.rows.iter_mut().filter(|r| r.is_active) {
                r.conditions.retain_mut(|c| match c.turns.as_mut() {
                    Some(t) => {
                        *t -= 1;
                        *t > 0
                    }
                    None => true,
                });
            }
        }
        let last = self.rows.iter().rposition(|r| r.is_active).unwrap_or(0);
        for r in &mut self.rows {
            r.is_active = false;
        }
        let next = if last + 1 < self.rows.len() { last + 1 } else { 0 };
        // the round starts at the top of the forward order
        let round_start = if direction == DIR_BACKWARDS {
            self.rows.reverse();
            let ix = self.rows.len() - 1 - next;
            self.set_active(ix, direction, true);
            false
        } else {
            self.set_active(next, direction, false)
        };
        self.sort_rows();
        if direction == DIR_FORWARDS && round_start && self.is_reroll_initiative_each_round {
            for r in &mut self.rows {
                if let Some(b) = bonus(r) {
                    r.initiative = Some(creature::roll_initiative(b, rng));
                }
                r.is_active = false;
            }
            self.sort_rows();
            self.set_active(0, direction, true);
        }
    }

    /// Adds a condition to a row (5etools condition picker).
    pub fn add_condition(
        &mut self,
        row_id: &str,
        name: &str,
        color: &str,
        turns: Option<i64>,
        new_id: NewId,
    ) -> bool {
        let Some(r) = self.rows.iter_mut().find(|r| r.id == row_id) else { return false };
        r.conditions.push(Condition {
            id: new_id(),
            name: name.into(),
            color: color.into(),
            turns: turns.filter(|t| *t > 0),
        });
        true
    }

    /// The active row(s), for status lines.
    pub fn active(&self) -> Vec<&Row> {
        self.rows.iter().filter(|r| r.is_active).collect()
    }

    // ── serial (5etools `_getSerializedState`) ──

    pub fn to_serial(&self) -> Value {
        let mut o = self.extra.clone();
        let mut put = |k: &str, v: Value| {
            o.insert(k.into(), v);
        };
        put("s", json!(self.sort));
        put("d", json!(self.dir));
        put("ri", json!(self.is_roll_init));
        put("m", json!(self.is_roll_hp));
        put("rg", json!(self.is_roll_groups));
        put("rri", json!(self.is_reroll_initiative_each_round));
        put("wId", json!(self.is_invert_wound_direction));
        put("g", json!(self.import_is_roll_groups));
        put("p", json!(self.import_is_add_players));
        put("a", json!(self.import_is_append));
        put("k", json!(self.is_stats_add_columns));
        put("piHp", json!(self.player_init_show_exact_player_hp));
        put("piHm", json!(self.player_init_show_exact_monster_hp));
        put("piV", json!(self.player_init_hide_new_monster));
        put("piO", json!(self.player_init_show_ordinals));
        put("nov", json!(self.is_notes_shown));
        put(
            "c",
            Value::Array(
                self.stats_cols
                    .iter()
                    .map(|c| {
                        json!({"id": c.id, "e": c.is_editable, "v": c.is_player_visible,
                               "p": c.populate_with, "a": c.abbreviation})
                    })
                    .collect(),
            ),
        );
        put(
            "cndc",
            Value::Array(
                self.conditions_custom
                    .iter()
                    .map(|c| {
                        strip_nulls(json!({"id": c.id, "n": c.name, "c": c.color, "t": c.turns}))
                    })
                    .collect(),
            ),
        );
        put("r", Value::Array(self.rows.iter().map(row_to_serial).collect()));
        put("rdp", Value::Array(self.rows_default_party.iter().map(row_to_serial).collect()));
        put("n", json!(self.round));
        put("not", json!(self.notes));
        Value::Object(o)
    }

    /// 5etools `_setStateFromSerialized`, with its legacy conversions.
    pub fn from_serial(v: &Value, new_id: NewId) -> Tracker {
        let mut t = Tracker::default();
        let Some(o) = v.as_object() else { return t };
        let b = |k: &str, d: bool| o.get(k).and_then(Value::as_bool).unwrap_or(d);
        if let Some(s) = o.get("s").and_then(|s| serde_json::from_value(s.clone()).ok()) {
            t.sort = s;
        }
        if let Some(d) = o.get("d").and_then(|s| serde_json::from_value(s.clone()).ok()) {
            t.dir = d;
        }
        t.is_roll_init = b("ri", t.is_roll_init);
        t.is_roll_hp = b("m", t.is_roll_hp);
        t.is_roll_groups = b("rg", t.is_roll_groups);
        t.is_reroll_initiative_each_round = b("rri", t.is_reroll_initiative_each_round);
        t.is_invert_wound_direction = b("wId", t.is_invert_wound_direction);
        t.import_is_roll_groups = b("g", t.import_is_roll_groups);
        t.import_is_add_players = b("p", t.import_is_add_players);
        t.import_is_append = b("a", t.import_is_append);
        t.is_stats_add_columns = b("k", t.is_stats_add_columns);
        t.player_init_show_exact_player_hp = b("piHp", t.player_init_show_exact_player_hp);
        t.player_init_show_exact_monster_hp = b("piHm", t.player_init_show_exact_monster_hp);
        t.player_init_hide_new_monster = b("piV", t.player_init_hide_new_monster);
        t.player_init_show_ordinals = b("piO", t.player_init_show_ordinals);
        t.is_notes_shown = b("nov", t.is_notes_shown);
        let arr = |k: &str| o.get(k).and_then(Value::as_array).cloned().unwrap_or_default();
        t.stats_cols = arr("c")
            .iter()
            .filter(|c| c.is_object())
            .map(|c| Column {
                id: str_of(c, "id").unwrap_or_else(&mut *new_id),
                is_editable: c.get("e").and_then(Value::as_bool).unwrap_or(true),
                is_player_visible: match c.get("v") {
                    Some(Value::Bool(b)) => u8::from(*b),
                    Some(v) => v.as_u64().unwrap_or(0).min(2) as u8,
                    None => 0,
                },
                populate_with: str_of(c, "p").unwrap_or_default(),
                abbreviation: str_of(c, "a").unwrap_or_default(),
            })
            .collect();
        t.conditions_custom = arr("cndc")
            .iter()
            .map(|c| CustomCondition {
                id: str_of(c, "id").unwrap_or_else(&mut *new_id),
                name: str_of(c, "n").unwrap_or_default(),
                color: str_of(c, "c").unwrap_or_else(|| "#888888".into()),
                turns: num_of(c, "t"),
            })
            .collect();
        t.rows = arr("r").iter().filter_map(|r| row_from_serial(r, new_id)).collect();
        t.rows_default_party =
            arr("rdp").iter().filter_map(|r| row_from_serial(r, new_id)).collect();
        t.round = o.get("n").and_then(num).unwrap_or(1).max(1);
        t.notes = str_of(v, "not").unwrap_or_default();
        const KNOWN: [&str; 21] = [
            "s", "d", "ri", "m", "rg", "rri", "wId", "g", "p", "a", "k", "piHp", "piHm", "piV",
            "piO", "nov", "c", "cndc", "r", "rdp", "n",
        ];
        t.extra = o
            .iter()
            .filter(|(k, _)| !KNOWN.contains(&k.as_str()) && k.as_str() != "not")
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        t
    }
}

fn strip_nulls(v: Value) -> Value {
    match v {
        Value::Object(o) => Value::Object(o.into_iter().filter(|(_, v)| !v.is_null()).collect()),
        v => v,
    }
}

fn str_of(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(Value::as_str).map(Into::into)
}

/// 5etools stores numbers as numbers or numeric strings.
fn num(v: &Value) -> Option<i64> {
    match v {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f.round() as i64)),
        Value::String(s) => s.trim().parse::<f64>().ok().map(|f| f.round() as i64),
        _ => None,
    }
}

fn num_of(v: &Value, k: &str) -> Option<i64> {
    v.get(k).and_then(num)
}

fn row_to_serial(r: &Row) -> Value {
    let conds: Vec<Value> = r
        .conditions
        .iter()
        .map(|c| json!({"id": c.id, "entity": strip_nulls(json!({"name": c.name, "color": c.color, "turns": c.turns}))}))
        .collect();
    let cells: Vec<Value> = r
        .cells
        .iter()
        .map(|c| {
            let e = &c.entity;
            strip_nulls(json!({
                "id": c.id,
                "v": e.get("value"), "cur": e.get("current"), "max": e.get("max"),
                "urlt": e.get("tokenUrl"), "hrefi": e.get("imageHref"),
            }))
        })
        .collect();
    strip_nulls(json!({
        "id": r.id,
        "n": r.name,
        "n_d": r.display_name,
        "n_scr": r.scaled_cr,
        "n_ssp": r.scaled_summon_spell_level,
        "n_scl": r.scaled_summon_class_level,
        "n_m": r.custom_name,
        "h": r.hp_current,
        "g": r.hp_max,
        "i": r.initiative,
        "a": r.is_active,
        "s": r.source,
        "c": conds,
        "nt": r.notes,
        "v": r.is_player_visible,
        "o": r.ordinal,
        "k": cells,
    }))
}

fn row_from_serial(v: &Value, new_id: NewId) -> Option<Row> {
    let o = v.as_object()?;
    // legacy: `n` was an object {n, d, scr, ssp, scl, m}
    let (name, legacy) = match o.get("n") {
        Some(Value::Object(n)) => {
            (str_of(&Value::Object(n.clone()), "n").unwrap_or_default(), Some(n))
        }
        Some(Value::String(s)) => (s.clone(), None),
        _ => (String::new(), None),
    };
    let pick = |k: &str, lk: &str| {
        o.get(k).or_else(|| legacy.and_then(|l| l.get(lk))).filter(|v| !v.is_null())
    };
    let conditions = o
        .get("c")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|c| {
            // legacy conditions had no id/entity wrapper
            let e = c.get("entity").unwrap_or(c);
            Condition {
                id: str_of(c, "id").unwrap_or_else(&mut *new_id),
                name: str_of(e, "name").unwrap_or_default(),
                color: str_of(e, "color").unwrap_or_else(|| "#888888".into()),
                turns: num_of(e, "turns"),
            }
        })
        .collect();
    let cells = o
        .get("k")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|c| {
            let mut e = Map::new();
            for (s, f) in [
                ("v", "value"),
                ("cur", "current"),
                ("max", "max"),
                ("urlt", "tokenUrl"),
                ("hrefi", "imageHref"),
            ] {
                if let Some(x) = c.get(s).filter(|x| !x.is_null()) {
                    e.insert(f.into(), x.clone());
                }
            }
            Some(Cell { id: str_of(c, "id")?, entity: e })
        })
        .collect();
    Some(Row {
        id: str_of(v, "id").unwrap_or_else(&mut *new_id),
        name,
        display_name: pick("n_d", "d").and_then(Value::as_str).map(Into::into),
        custom_name: pick("n_m", "m").and_then(Value::as_str).map(Into::into),
        source: str_of(v, "s").filter(|s| !s.is_empty()),
        scaled_cr: pick("n_scr", "scr")
            .and_then(|v| v.as_f64().or_else(|| v.as_str()?.parse().ok())),
        scaled_summon_spell_level: pick("n_ssp", "ssp").and_then(num),
        scaled_summon_class_level: pick("n_scl", "scl").and_then(num),
        hp_current: num_of(v, "h"),
        hp_max: num_of(v, "g"),
        initiative: num_of(v, "i"),
        is_active: o
            .get("a")
            .is_some_and(|a| a.as_bool().unwrap_or(a.as_i64().is_some_and(|n| n != 0))),
        conditions,
        notes: str_of(v, "nt").unwrap_or_default(),
        is_player_visible: o.get("v").and_then(Value::as_bool).unwrap_or(true),
        ordinal: num_of(v, "o").unwrap_or(1),
        cells,
    })
}

#[cfg(test)]
mod tests;
