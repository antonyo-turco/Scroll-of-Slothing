//! 5etools spells (`data/spells/*.json`) and items (`data/items*.json`,
//! `data/magicvariants.json`) as card markdown for the card printer.
//! Spec: `docs/features/spell-item-cards.md`.

use std::collections::HashMap;
use std::sync::OnceLock;

use regex::Regex;
use serde_json::{Map, Value};

/// Item rarities in 5etools order, with the banner colour of their cards.
pub const RARITIES: [(&str, Option<&str>); 9] = [
    ("none", None),
    ("common", Some("#5C5C5C")),
    ("uncommon", Some("#2E7D32")),
    ("rare", Some("#1F5FAD")),
    ("very rare", Some("#6A35A8")),
    ("legendary", Some("#B35C00")),
    ("artifact", Some("#A8322B")),
    ("varies", None),
    ("unknown", None),
];

const SCHOOLS: [(&str, &str); 8] = [
    ("A", "Abjuration"),
    ("C", "Conjuration"),
    ("D", "Divination"),
    ("E", "Enchantment"),
    ("V", "Evocation"),
    ("I", "Illusion"),
    ("N", "Necromancy"),
    ("T", "Transmutation"),
];

const DAMAGE_TYPES: [(&str, &str); 13] = [
    ("A", "acid"),
    ("B", "bludgeoning"),
    ("C", "cold"),
    ("F", "fire"),
    ("O", "force"),
    ("L", "lightning"),
    ("N", "necrotic"),
    ("P", "piercing"),
    ("I", "poison"),
    ("Y", "psychic"),
    ("R", "radiant"),
    ("S", "slashing"),
    ("T", "thunder"),
];

fn lookup<'a>(table: &[(&str, &'a str)], key: &str) -> Option<&'a str> {
    table.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| *v)
}

fn str_of(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(Into::into)
}

/// `"M|XPHB"` → `"M"`.
fn strip_source(s: &str) -> &str {
    s.split('|').next().unwrap_or(s).trim()
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

fn title_case(s: &str) -> String {
    s.split(' ').map(capitalize).collect::<Vec<_>>().join(" ")
}

fn ordinal(n: i64) -> String {
    let suffix = match (n % 10, n % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

fn plural(n: i64, unit: &str) -> String {
    if n == 1 { format!("{n} {unit}") } else { format!("{n} {unit}s") }
}

/// Keeps generated text from changing the card format: `*` and `_` would
/// become styling (or be dropped), and a line of only dashes splits cards.
fn clean(s: &str) -> String {
    let s: String = s.chars().filter(|c| *c != '*' && *c != '_').collect();
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if !s.is_empty() && s.chars().all(|c| c == '-') { String::new() } else { s }
}

// ── 5etools tags ──

/// The text a 5etools tag shows: `{@spell fireball|XPHB}` → `fireball`,
/// `{@variantrule Sphere [Area of Effect]|XPHB|Sphere}` → `Sphere`,
/// `{@damage 8d6}` → `8d6`, `{@dc 15}` → `DC 15`. Tags nest, innermost first.
/// `{@b …}` and `{@i …}` become card bold and italic.
pub fn plain(s: &str) -> String {
    static TAG: OnceLock<Regex> = OnceLock::new();
    let re = TAG.get_or_init(|| Regex::new(r"\{@(\w+)\s*([^{}]*)\}").expect("valid regex"));
    let mut out = s.to_string();
    for _ in 0..8 {
        if !out.contains("{@") {
            break;
        }
        out = re.replace_all(&out, |c: &regex::Captures| tag_text(&c[1], &c[2])).into_owned();
    }
    out
}

fn tag_text(tag: &str, body: &str) -> String {
    let parts: Vec<&str> = body.split('|').map(str::trim).collect();
    let first = parts.first().copied().unwrap_or("");
    let at = |i: usize| parts.get(i).copied().filter(|p| !p.is_empty());
    match tag {
        "b" | "bold" => format!("**{first}**"),
        "i" | "italic" => format!("*{first}*"),
        "hit" => match first.parse::<i64>() {
            Ok(n) if n >= 0 => format!("+{n}"),
            _ => first.into(),
        },
        "h" => "Hit: ".into(),
        "dc" => format!("DC {first}"),
        "chance" => at(1).map(Into::into).unwrap_or_else(|| format!("{first} percent")),
        "recharge" => match first {
            "" | "6" => "(Recharge 6)".into(),
            n => format!("(Recharge {n}–6)"),
        },
        "atk" | "atkr" => match first {
            "mw" => "Melee Weapon Attack:",
            "rw" => "Ranged Weapon Attack:",
            "mw,rw" => "Melee or Ranged Weapon Attack:",
            "ms" => "Melee Spell Attack:",
            "rs" => "Ranged Spell Attack:",
            "m" => "Melee Attack Roll:",
            "r" => "Ranged Attack Roll:",
            _ => "",
        }
        .into(),
        // the extra dice per slot level
        "scaledamage" | "scaledice" => at(2).unwrap_or(first).into(),
        "dice" | "damage" | "d20" | "note" | "link" | "color" | "filter" | "book" | "adventure"
        | "5etools" => first.into(),
        "quickref" => at(4).unwrap_or(first).into(),
        "classFeature" => at(5).unwrap_or(first).into(),
        "subclassFeature" => at(7).unwrap_or(first).into(),
        "card" => at(3).unwrap_or(first).into(),
        // links: name|source|display; drop a "[Area of Effect]" qualifier
        _ => at(2).map(Into::into).unwrap_or_else(|| match first.find(" [") {
            Some(i) if first.ends_with(']') => first[..i].to_string(),
            _ => first.to_string(),
        }),
    }
}

/// `{=bonusWeapon}` placeholders of magic variants, from the variant's fields.
/// `/l` lower-cases the value, `/a` puts "a" or "an" in front and `/at` "A" or "An". A
/// generic variant has no base item, so `baseName` is "weapon", "armor" or
/// "ammunition" from what it can be made from.
fn fill_placeholders(s: &str, item: &Value) -> String {
    static VAR: OnceLock<Regex> = OnceLock::new();
    let re = VAR.get_or_init(|| Regex::new(r"\{=(\w+)(?:/(\w+))?\}").expect("valid regex"));
    re.replace_all(s, |c: &regex::Captures| {
        let v = match (&c[1], item.get(&c[1])) {
            ("dmgType", Some(Value::String(t))) => lookup(&DAMAGE_TYPES, t).unwrap_or(t).to_string(),
            (_, Some(Value::String(v))) => v.clone(),
            (_, Some(Value::Number(n))) => n.to_string(),
            ("baseName", _) => generic_base(item).to_string(),
            _ => String::new(),
        };
        let article = |v: &str| {
            let an = v.starts_with(|ch: char| "aeiouAEIOU".contains(ch));
            format!("{} {v}", if an { "an" } else { "a" })
        };
        match c.get(2).map(|m| m.as_str()) {
            Some("l") => v.to_lowercase(),
            Some("a") => article(&v.to_lowercase()),
            Some("at") => capitalize(&article(&v.to_lowercase())),
            _ => v,
        }
    })
    .into_owned()
}

fn generic_base(item: &Value) -> &'static str {
    let req = item.get("requires").and_then(Value::as_array);
    let has = |k: &str| req.into_iter().flatten().any(|r| r.get(k).is_some());
    let ammo = req.into_iter().flatten().any(|r| {
        r.get("type").and_then(Value::as_str).is_some_and(|t| matches!(strip_source(t), "A" | "AF"))
    });
    if ammo || item.get("ammo").and_then(Value::as_bool) == Some(true) {
        "ammunition"
    } else if has("armor") || has("type") && !has("weapon") {
        "armor"
    } else {
        "weapon"
    }
}

/// Fills placeholders in every string of a 5etools value (before tags are read).
fn fill_all(v: &Value, item: &Value) -> Value {
    match v {
        Value::String(s) => Value::String(fill_placeholders(s, item)),
        Value::Array(a) => Value::Array(a.iter().map(|x| fill_all(x, item)).collect()),
        Value::Object(o) => {
            Value::Object(o.iter().map(|(k, x)| (k.clone(), fill_all(x, item))).collect())
        }
        other => other.clone(),
    }
}

// ── entries ──

/// 5etools `entries` as card paragraphs: named blocks start with a bold
/// "**Name.**", list items are bulleted, and table rows become one line each.
pub fn paragraphs(entries: &Value) -> Vec<String> {
    let mut out = vec![];
    if let Some(a) = entries.as_array() {
        for e in a {
            entry(e, &mut out);
        }
    } else {
        entry(entries, &mut out);
    }
    out.retain(|p| !p.is_empty());
    out
}

fn text(s: &str) -> String {
    clean_keep_style(&plain(s))
}

/// Like [`clean`], but keeps the `**`/`*` that [`plain`] made from `{@b}`/`{@i}`.
fn clean_keep_style(s: &str) -> String {
    let s: String = s.chars().filter(|c| *c != '_').collect();
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if !s.is_empty() && s.chars().all(|c| c == '-') { String::new() } else { s }
}

fn bold_name(name: &str) -> String {
    let n = clean(&plain(name));
    if n.is_empty() {
        return n;
    }
    let dot = if n.ends_with(['.', '!', '?', ':']) { "" } else { "." };
    format!("**{n}{dot}**")
}

/// Pushes `body` with `lead` ("**Name.**" or "•") in front of its first paragraph.
fn lead_into(lead: String, body: Vec<String>, out: &mut Vec<String>) {
    let mut it = body.into_iter();
    match (lead.is_empty(), it.next()) {
        (true, Some(first)) => out.push(first),
        (false, Some(first)) => out.push(format!("{lead} {first}")),
        (false, None) => out.push(lead),
        (true, None) => {}
    }
    out.extend(it);
}

fn entry(e: &Value, out: &mut Vec<String>) {
    match e {
        Value::String(s) => out.push(text(s)),
        Value::Object(o) => {
            let kind = o.get("type").and_then(Value::as_str).unwrap_or("entries");
            let name = o.get("name").and_then(Value::as_str).map(bold_name).unwrap_or_default();
            match kind {
                "list" => {
                    for item in o.get("items").and_then(Value::as_array).into_iter().flatten() {
                        let mut body = vec![];
                        entry(item, &mut body);
                        lead_into("•".into(), body, out);
                    }
                }
                "item" | "itemSub" | "itemSpell" => {
                    let mut body = vec![];
                    if let Some(x) = o.get("entry") {
                        entry(x, &mut body);
                    }
                    if let Some(x) = o.get("entries") {
                        body.extend(paragraphs(x));
                    }
                    lead_into(name, body, out);
                }
                "table" => table(o, out),
                "quote" => {
                    let mut body = paragraphs(o.get("entries").unwrap_or(&Value::Null));
                    if let Some(by) = o.get("by").and_then(Value::as_str) {
                        body.push(format!("— {}", text(by)));
                    }
                    out.extend(body);
                }
                "abilityDc" => {
                    let attrs = attributes(o);
                    out.push(format!(
                        "**{} save DC** = 8 + your proficiency bonus + your {attrs} modifier",
                        clean(o.get("name").and_then(Value::as_str).unwrap_or("Spell"))
                    ));
                }
                "abilityAttackMod" => {
                    let attrs = attributes(o);
                    out.push(format!(
                        "**{} attack modifier** = your proficiency bonus + your {attrs} modifier",
                        clean(o.get("name").and_then(Value::as_str).unwrap_or("Spell"))
                    ));
                }
                _ => {
                    let mut body = vec![];
                    if let Some(x) = o.get("entries") {
                        body.extend(paragraphs(x));
                    } else if let Some(x) = o.get("entry") {
                        entry(x, &mut body);
                    }
                    lead_into(name, body, out);
                }
            }
        }
        Value::Array(a) => a.iter().for_each(|x| entry(x, out)),
        Value::Number(n) => out.push(n.to_string()),
        _ => {}
    }
}

fn attributes(o: &Map<String, Value>) -> String {
    let names: Vec<String> = o
        .get("attributes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(|a| match a {
            "str" => "Strength",
            "dex" => "Dexterity",
            "con" => "Constitution",
            "int" => "Intelligence",
            "wis" => "Wisdom",
            "cha" => "Charisma",
            other => other,
        })
        .map(Into::into)
        .collect();
    names.join(" or ")
}

fn cell(c: &Value) -> String {
    match c {
        Value::String(s) => text(s),
        Value::Number(n) => n.to_string(),
        Value::Object(o) if o.get("type").and_then(Value::as_str) == Some("cell") => {
            let roll = o.get("roll").unwrap_or(&Value::Null);
            let pad = roll.get("pad").and_then(Value::as_bool).unwrap_or(false);
            let num = |k: &str| {
                roll.get(k).and_then(Value::as_i64).map(|n| {
                    if pad { format!("{n:02}") } else { n.to_string() }
                })
            };
            match (num("exact"), num("min"), num("max")) {
                (Some(x), _, _) => x,
                (_, Some(a), Some(b)) => format!("{a}–{b}"),
                _ => o.get("entry").map(cell).unwrap_or_default(),
            }
        }
        other => paragraphs(other).join(" "),
    }
}

fn table(o: &Map<String, Value>, out: &mut Vec<String>) {
    if let Some(c) = o.get("caption").and_then(Value::as_str) {
        out.push(format!("**{}**", clean(&plain(c))));
    }
    let join = |cells: Vec<String>| {
        if cells.len() == 2 { cells.join(" — ") } else { cells.join(" · ") }
    };
    if let Some(labels) = o.get("colLabels").and_then(Value::as_array) {
        let l: Vec<String> = labels.iter().map(|x| clean(&plain(x.as_str().unwrap_or("")))).collect();
        if l.iter().any(|x| !x.is_empty()) {
            out.push(format!("**{}**", join(l)));
        }
    }
    for row in o.get("rows").and_then(Value::as_array).into_iter().flatten() {
        let cells = row.get("row").unwrap_or(row);
        if let Some(a) = cells.as_array() {
            out.push(join(a.iter().map(cell).collect()));
        }
    }
}

// ── spells ──

/// `"3rd-level Evocation"`, `"Evocation cantrip"`; `(ritual)` when it is one.
pub fn spell_level_school(spell: &Value) -> String {
    let level = spell.get("level").and_then(Value::as_i64).unwrap_or(0);
    let school = spell
        .get("school")
        .and_then(Value::as_str)
        .map(|s| lookup(&SCHOOLS, s).unwrap_or(s).to_string())
        .unwrap_or_default();
    let mut s = if level == 0 {
        format!("{school} cantrip").trim().to_string()
    } else {
        format!("{}-level {school}", ordinal(level)).trim().to_string()
    };
    if spell.pointer("/meta/ritual").and_then(Value::as_bool) == Some(true) {
        s.push_str(" (ritual)");
    }
    s
}

pub fn spell_concentration(spell: &Value) -> bool {
    spell
        .get("duration")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|d| d.get("concentration").and_then(Value::as_bool) == Some(true))
}

fn spell_time(spell: &Value) -> String {
    let times: Vec<String> = spell
        .get("time")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|t| {
            let n = t.get("number").and_then(Value::as_i64).unwrap_or(1);
            match t.get("unit").and_then(Value::as_str).unwrap_or("") {
                "bonus" => plural(n, "bonus action"),
                "action" => plural(n, "action"),
                "reaction" => plural(n, "reaction"),
                u => plural(n, u),
            }
        })
        .collect();
    times.join(" or ")
}

fn distance(d: &Value) -> String {
    let amount = d.get("amount").and_then(Value::as_i64);
    match (d.get("type").and_then(Value::as_str).unwrap_or(""), amount) {
        ("feet", Some(n)) => format!("{n} feet"),
        ("miles", Some(n)) => plural(n, "mile"),
        ("self", _) => "Self".into(),
        ("touch", _) => "Touch".into(),
        ("sight", _) => "Sight".into(),
        ("unlimited", _) => "Unlimited".into(),
        ("plane", _) => "Special".into(),
        (t, _) => capitalize(t),
    }
}

fn spell_range(spell: &Value) -> String {
    let r = spell.get("range").unwrap_or(&Value::Null);
    let d = r.get("distance").unwrap_or(&Value::Null);
    match r.get("type").and_then(Value::as_str).unwrap_or("") {
        "point" => distance(d),
        "special" => "Special".into(),
        shape => {
            let n = d.get("amount").and_then(Value::as_i64).unwrap_or(0);
            let unit = match d.get("type").and_then(Value::as_str) {
                Some("miles") => "mile",
                _ => "foot",
            };
            format!("Self ({n}-{unit} {shape})")
        }
    }
}

fn spell_components(spell: &Value) -> String {
    let c = spell.get("components").unwrap_or(&Value::Null);
    let mut parts = vec![];
    for (k, label) in [("v", "V"), ("s", "S")] {
        if c.get(k).and_then(Value::as_bool) == Some(true) {
            parts.push(label.to_string());
        }
    }
    match c.get("m") {
        Some(Value::String(m)) => parts.push(format!("M ({})", clean(&plain(m)))),
        Some(Value::Object(m)) => parts.push(format!(
            "M ({})",
            clean(&plain(m.get("text").and_then(Value::as_str).unwrap_or("")))
        )),
        Some(Value::Bool(true)) => parts.push("M".into()),
        _ => {}
    }
    if c.get("r").and_then(Value::as_bool) == Some(true) {
        parts.push("R".into());
    }
    parts.join(", ")
}

fn spell_duration(spell: &Value) -> String {
    let durations: Vec<String> = spell
        .get("duration")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|d| match d.get("type").and_then(Value::as_str).unwrap_or("") {
            "instant" => "Instantaneous".into(),
            "timed" => {
                let t = d.get("duration").unwrap_or(&Value::Null);
                let n = t.get("amount").and_then(Value::as_i64).unwrap_or(1);
                let span = plural(n, t.get("type").and_then(Value::as_str).unwrap_or("round"));
                if d.get("concentration").and_then(Value::as_bool) == Some(true) {
                    format!("Concentration, up to {span}")
                } else if t.get("upTo").and_then(Value::as_bool) == Some(true) {
                    format!("Up to {span}")
                } else {
                    span
                }
            }
            "permanent" => {
                let ends: Vec<&str> = d
                    .get("ends")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(|e| match e {
                        "dispel" => "dispelled",
                        "trigger" => "triggered",
                        "discharge" => "discharged",
                        other => other,
                    })
                    .collect();
                if ends.is_empty() {
                    "Permanent".into()
                } else {
                    format!("Until {}", ends.join(" or "))
                }
            }
            _ => "Special".into(),
        })
        .collect();
    durations.join(" or ")
}

/// A spell as one card: level and school, casting time, range, components,
/// duration and classes as stats, then its text and higher-level text.
pub fn spell_card(spell: &Value, classes: &[String]) -> String {
    let mut md = format!("# {}\n**Type:** Spell\n", clean(&str_of(spell, "name").unwrap_or_default()));
    let mut stat = |k: &str, v: String| {
        if !v.is_empty() {
            md.push_str(&format!("**{k}:** {v}\n"));
        }
    };
    stat("Level", spell_level_school(spell));
    stat("Casting Time", spell_time(spell));
    stat("Range", spell_range(spell));
    stat("Components", spell_components(spell));
    stat("Duration", spell_duration(spell));
    stat("Classes", classes.join(", "));
    let mut body = paragraphs(spell.get("entries").unwrap_or(&Value::Null));
    if let Some(h) = spell.get("entriesHigherLevel") {
        body.extend(paragraphs(h));
    }
    finish(md, body)
}

fn finish(mut md: String, body: Vec<String>) -> String {
    if !body.is_empty() {
        md.push('\n');
        md.push_str(&body.join("\n\n"));
    }
    md.trim_end().to_string()
}

// ── items ──

/// Item type and property names from `items-base.json` (`itemType`,
/// `itemProperty`), by abbreviation.
#[derive(Debug, Clone, Default)]
pub struct ItemRefs {
    pub types: HashMap<String, String>,
    pub properties: HashMap<String, String>,
}

impl ItemRefs {
    pub fn from_base(base: &Value) -> ItemRefs {
        let mut refs = ItemRefs::default();
        let name_of = |x: &Value| {
            x.get("name")
                .and_then(Value::as_str)
                .or_else(|| x.pointer("/entries/0/name").and_then(Value::as_str))
                .map(String::from)
        };
        for (key, map) in [("itemType", &mut refs.types), ("itemProperty", &mut refs.properties)] {
            for x in base.get(key).and_then(Value::as_array).into_iter().flatten() {
                if let (Some(a), Some(n)) = (x.get("abbreviation").and_then(Value::as_str), name_of(x)) {
                    // the first (older) printing wins only when no newer one names it
                    map.insert(a.to_uppercase(), n);
                }
            }
        }
        refs
    }
}

pub fn item_rarity(item: &Value) -> String {
    item.get("rarity").and_then(Value::as_str).unwrap_or("none").to_lowercase()
}

fn rarity_color(rarity: &str) -> Option<&'static str> {
    RARITIES.iter().find(|(r, _)| *r == rarity).and_then(|(_, c)| *c)
}

/// `"Wondrous item"`, `"Martial Weapon (longsword)"`, `"Potion"`, `"Weapon (any sword)"`.
pub fn item_kind(item: &Value, refs: &ItemRefs) -> String {
    let base = item
        .get("baseItem")
        .and_then(Value::as_str)
        .map(|b| strip_source(b).to_lowercase());
    let type_name = item.get("type").and_then(Value::as_str).map(|t| {
        let abbr = strip_source(t).to_uppercase();
        refs.types.get(&abbr).cloned().unwrap_or(abbr)
    });
    let mut kind = if item.get("wondrous").and_then(Value::as_bool) == Some(true) {
        "Wondrous item".to_string()
    } else if item.get("staff").and_then(Value::as_bool) == Some(true) {
        "Staff".to_string()
    } else if let Some(req) = item.get("requires").and_then(Value::as_array) {
        // generic magic variants: what they can be made from
        let mut kinds: Vec<String> = vec![];
        for r in req {
            let k = if let Some(t) = r.get("type").and_then(Value::as_str) {
                let abbr = strip_source(t).to_uppercase();
                refs.types.get(&abbr).cloned().unwrap_or(abbr)
            } else if let Some((k, _)) = r.as_object().and_then(|o| o.iter().next()) {
                match k.as_str() {
                    "weapon" => "Weapon (any)".into(),
                    "armor" => "Armor (any)".into(),
                    other => format!("Weapon (any {other})"),
                }
            } else {
                continue;
            };
            if !kinds.contains(&k) {
                kinds.push(k);
            }
        }
        kinds.join(" or ")
    } else if let Some(cat) = item.get("weaponCategory").and_then(Value::as_str) {
        format!("{} weapon", capitalize(cat))
    } else {
        type_name.clone().unwrap_or_default()
    };
    if let Some(b) = base
        && !kind.is_empty()
    {
        kind = format!("{kind} ({b})");
    }
    if item.get("tattoo").and_then(Value::as_bool) == Some(true) && kind.is_empty() {
        kind = "Tattoo".into();
    }
    kind
}

fn item_attunement(item: &Value) -> String {
    match item.get("reqAttune") {
        Some(Value::Bool(true)) => "Required".into(),
        Some(Value::String(s)) if !s.is_empty() => format!("Required ({})", clean(&plain(s))),
        _ => String::new(),
    }
}

fn item_damage(item: &Value) -> String {
    let Some(d1) = item.get("dmg1").and_then(Value::as_str) else { return String::new() };
    let kind = item
        .get("dmgType")
        .and_then(Value::as_str)
        .and_then(|t| lookup(&DAMAGE_TYPES, t))
        .unwrap_or("");
    let mut s = format!("{d1} {kind}").trim().to_string();
    if let Some(d2) = item.get("dmg2").and_then(Value::as_str) {
        s.push_str(&format!(" ({d2} two-handed)"));
    }
    if let Some(b) = item.get("bonusWeapon").and_then(Value::as_str) {
        s = format!("{s}, {b} to attack and damage");
    }
    s
}

fn item_properties(item: &Value, refs: &ItemRefs) -> String {
    let mut names = vec![];
    for p in item.get("property").and_then(Value::as_array).into_iter().flatten() {
        let (abbr, note) = match p {
            Value::String(s) => (s.as_str(), None),
            Value::Object(o) => (
                o.get("uid").and_then(Value::as_str).unwrap_or(""),
                o.get("note").and_then(Value::as_str),
            ),
            _ => continue,
        };
        let abbr = strip_source(abbr).to_uppercase();
        let mut n = refs.properties.get(&abbr).cloned().unwrap_or(abbr);
        if let Some(note) = note {
            n = format!("{n} ({})", clean(&plain(note)));
        }
        if (n == "Thrown" || n == "Ammunition")
            && let Some(r) = item.get("range").and_then(Value::as_str)
        {
            n = format!("{n} ({r} ft.)");
        }
        names.push(n);
    }
    if let Some(m) = item.get("mastery").and_then(Value::as_array) {
        let m: Vec<&str> = m.iter().filter_map(Value::as_str).map(strip_source).collect();
        if !m.is_empty() {
            names.push(format!("Mastery: {}", m.join(", ")));
        }
    }
    names.join(", ")
}

fn item_ac(item: &Value) -> String {
    let mut s = match item.get("ac").and_then(Value::as_i64) {
        Some(ac) if strip_source(item.get("type").and_then(Value::as_str).unwrap_or("")) == "S" => {
            format!("+{ac}")
        }
        Some(ac) => match strip_source(item.get("type").and_then(Value::as_str).unwrap_or("")) {
            "LA" => format!("{ac} + Dex modifier"),
            "MA" => format!("{ac} + Dex modifier (max 2)"),
            _ => ac.to_string(),
        },
        None => String::new(),
    };
    if let Some(b) = item.get("bonusAc").and_then(Value::as_str) {
        s = if s.is_empty() { format!("{b} bonus") } else { format!("{s}, {b} bonus") };
    }
    s
}

/// `value` is in copper pieces.
fn item_value(item: &Value) -> String {
    let Some(cp) = item.get("value").and_then(Value::as_f64) else { return String::new() };
    if cp <= 0.0 {
        return String::new();
    }
    let (n, unit) = if cp >= 100.0 && (cp / 100.0).fract() == 0.0 {
        (cp / 100.0, "gp")
    } else if cp >= 10.0 && (cp / 10.0).fract() == 0.0 {
        (cp / 10.0, "sp")
    } else {
        (cp, "cp")
    };
    format!("{} {unit}", group_thousands(n as i64))
}

fn group_thousands(n: i64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn item_weight(item: &Value) -> String {
    match item.get("weight").and_then(Value::as_f64) {
        Some(w) if w > 0.0 => {
            let w = if w.fract() == 0.0 { format!("{}", w as i64) } else { format!("{w}") };
            format!("{w} lb.")
        }
        _ => String::new(),
    }
}

/// An item as one card: kind, rarity, attunement and the weapon, armor,
/// charge, weight and value stats it has, then its text. The banner takes the
/// rarity's colour.
pub fn item_card(item: &Value, refs: &ItemRefs) -> String {
    let rarity = item_rarity(item);
    let kind_line = match rarity_color(&rarity) {
        Some(hex) => format!("**Type:** Item {hex}\n"),
        None => "**Type:** Item\n".to_string(),
    };
    let mut md = format!("# {}\n{kind_line}", clean(&str_of(item, "name").unwrap_or_default()));
    let mut stat = |k: &str, v: String| {
        if !v.is_empty() {
            md.push_str(&format!("**{k}:** {v}\n"));
        }
    };
    stat("Kind", item_kind(item, refs));
    if rarity != "none" {
        stat("Rarity", title_case(&rarity));
    }
    stat("Attunement", item_attunement(item));
    stat("Damage", item_damage(item));
    stat("AC", item_ac(item));
    stat("Properties", item_properties(item, refs));
    if let Some(c) = item.get("charges").and_then(Value::as_i64) {
        let mut s = c.to_string();
        if let Some(r) = item.get("recharge").and_then(Value::as_str) {
            let when = match r {
                "dawn" => "at dawn",
                "dusk" => "at dusk",
                "midnight" => "at midnight",
                "restLong" => "on a long rest",
                "restShort" => "on a short rest",
                other => other,
            };
            let amount = item.get("rechargeAmount").map(|a| match a {
                Value::String(s) => format!("{} ", plain(s)),
                Value::Number(n) => format!("{n} "),
                _ => String::new(),
            });
            s = format!("{s} (regains {}{when})", amount.unwrap_or_default());
        }
        stat("Charges", s);
    }
    stat("Weight", item_weight(item));
    stat("Value", item_value(item));
    let entries = fill_all(item.get("entries").unwrap_or(&Value::Null), item);
    finish(md, paragraphs(&entries))
}

/// A generic magic variant (`magicvariants.json`, e.g. "+1 Weapon") as an item:
/// its `inherits` fields over its own, with `{=…}` filled in later.
pub fn variant_as_item(variant: &Value) -> Value {
    let mut out = variant.as_object().cloned().unwrap_or_default();
    let inherits = out.remove("inherits");
    for k in ["type", "_copy"] {
        out.remove(k);
    }
    if let Some(Value::Object(i)) = inherits {
        for (k, v) in i {
            if k != "namePrefix" && k != "nameSuffix" && k != "nameRemove" {
                out.insert(k, v);
            }
        }
    }
    Value::Object(out)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::cards::parse;

    fn fireball() -> Value {
        json!({
            "name": "Fireball", "source": "XPHB", "level": 3, "school": "V",
            "time": [{"number": 1, "unit": "action"}],
            "range": {"type": "point", "distance": {"type": "feet", "amount": 150}},
            "components": {"v": true, "s": true, "m": "a ball of bat guano and sulfur"},
            "duration": [{"type": "instant"}],
            "entries": [
                "Each creature in a 20-foot-radius {@variantrule Sphere [Area of Effect]|XPHB|Sphere} makes a Dexterity saving throw, taking {@damage 8d6} Fire damage.",
                "Flammable objects start {@hazard burning|XPHB}."
            ],
            "entriesHigherLevel": [{"type": "entries", "name": "Using a Higher-Level Spell Slot",
                "entries": ["The damage increases by {@scaledamage 8d6|3-9|1d6} for each spell slot level above 3."]}]
        })
    }

    #[test]
    fn tags_show_their_text() {
        assert_eq!(plain("{@spell fire bolt|XPHB}"), "fire bolt");
        assert_eq!(plain("{@variantrule Sphere [Area of Effect]|XPHB|Sphere}"), "Sphere");
        assert_eq!(plain("{@variantrule Hit Points|XPHB}"), "Hit Points");
        assert_eq!(plain("{@damage 2d6 + 3}"), "2d6 + 3");
        assert_eq!(plain("{@dc 15}"), "DC 15");
        assert_eq!(plain("{@hit 5} to hit"), "+5 to hit");
        assert_eq!(plain("{@chance 25}"), "25 percent");
        assert_eq!(plain("{@quickref Cover||3||cover}"), "cover");
        assert_eq!(plain("{@note see {@spell light}}"), "see light");
        assert_eq!(plain("{@b Bold} and {@i it}"), "**Bold** and *it*");
    }

    #[test]
    fn spell_cards_parse_back() {
        let md = spell_card(&fireball(), &["Sorcerer".into(), "Wizard".into()]);
        assert!(md.starts_with("# Fireball\n**Type:** Spell\n**Level:** 3rd-level Evocation\n"), "{md}");
        let c = &parse(&md)[0];
        assert_eq!(c.title, "Fireball");
        assert_eq!(c.kind, "Spell");
        let stat = |k: &str| c.stats.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
        assert_eq!(stat("Casting Time"), Some("1 action"));
        assert_eq!(stat("Range"), Some("150 feet"));
        assert_eq!(stat("Components"), Some("V, S, M (a ball of bat guano and sulfur)"));
        assert_eq!(stat("Duration"), Some("Instantaneous"));
        assert_eq!(stat("Classes"), Some("Sorcerer, Wizard"));
        let paras: Vec<&str> = c.body.split('\n').collect();
        assert_eq!(paras.len(), 3);
        assert!(paras[0].contains("20-foot-radius Sphere makes") && paras[0].contains("8d6 Fire"));
        assert_eq!(
            paras[2],
            "**Using a Higher-Level Spell Slot.** The damage increases by 1d6 for each spell slot level above 3."
        );
    }

    #[test]
    fn spell_lines_cover_the_variants() {
        let s = json!({"level": 0, "school": "C", "meta": {"ritual": true},
            "time": [{"number": 1, "unit": "bonus"}],
            "range": {"type": "cone", "distance": {"type": "feet", "amount": 15}},
            "components": {"v": true, "m": {"text": "gold dust worth 25+ GP", "cost": 2500}},
            "duration": [{"type": "timed", "concentration": true, "duration": {"type": "minute", "amount": 10}}]});
        assert_eq!(spell_level_school(&s), "Conjuration cantrip (ritual)");
        assert_eq!(spell_time(&s), "1 bonus action");
        assert_eq!(spell_range(&s), "Self (15-foot cone)");
        assert_eq!(spell_components(&s), "V, M (gold dust worth 25+ GP)");
        assert_eq!(spell_duration(&s), "Concentration, up to 10 minutes");
        assert!(spell_concentration(&s));
        let p = json!({"duration": [{"type": "permanent", "ends": ["dispel", "trigger"]}],
            "range": {"type": "point", "distance": {"type": "touch"}}});
        assert_eq!(spell_duration(&p), "Until dispelled or triggered");
        assert_eq!(spell_range(&p), "Touch");
        assert_eq!(ordinal(2), "2nd");
        assert_eq!(ordinal(11), "11th");
    }

    fn refs() -> ItemRefs {
        ItemRefs::from_base(&json!({
            "itemType": [{"abbreviation": "M", "name": "Melee Weapon"}, {"abbreviation": "P", "name": "Potion"},
                         {"abbreviation": "HA", "name": "Heavy Armor"}],
            "itemProperty": [{"abbreviation": "V", "entries": [{"type": "entries", "name": "Versatile"}]},
                             {"abbreviation": "T", "name": "Thrown"}]
        }))
    }

    #[test]
    fn item_cards_take_the_rarity_colour() {
        let bag = json!({"name": "Bag of Holding", "source": "XDMG", "rarity": "uncommon", "wondrous": true,
            "weight": 5, "entries": ["Retrieving an item requires a {@action Utilize|XPHB} action."]});
        let md = item_card(&bag, &refs());
        let c = &parse(&md)[0];
        assert_eq!(c.kind, "Item");
        assert_eq!(c.color, [0x2E as f64 / 255.0, 0x7D as f64 / 255.0, 0x32 as f64 / 255.0]);
        let stat = |k: &str| c.stats.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
        assert_eq!(stat("Kind").as_deref(), Some("Wondrous item"));
        assert_eq!(stat("Rarity").as_deref(), Some("Uncommon"));
        assert_eq!(stat("Weight").as_deref(), Some("5 lb."));
        assert_eq!(c.body, "Retrieving an item requires a Utilize action.");
    }

    #[test]
    fn weapons_list_damage_and_properties() {
        let sword = json!({"name": "Longsword", "type": "M|XPHB", "rarity": "none", "weaponCategory": "martial",
            "property": ["V|XPHB"], "mastery": ["Sap|XPHB"], "dmg1": "1d8", "dmgType": "S", "dmg2": "1d10",
            "weight": 3, "value": 1500});
        let md = item_card(&sword, &refs());
        assert!(md.contains("**Type:** Item\n"), "{md}");
        assert!(md.contains("**Kind:** Martial weapon\n"));
        assert!(!md.contains("Rarity"));
        assert!(md.contains("**Damage:** 1d8 slashing (1d10 two-handed)\n"));
        assert!(md.contains("**Properties:** Versatile, Mastery: Sap\n"));
        assert!(md.contains("**Value:** 15 gp"));
        let plate = json!({"name": "Plate", "type": "HA", "ac": 18, "value": 150000});
        assert_eq!(item_ac(&plate), "18");
        assert_eq!(item_value(&plate), "1,500 gp");
    }

    #[test]
    fn magic_variants_fill_their_placeholders() {
        let v = json!({"name": "+1 Weapon", "type": "GV|DMG", "requires": [{"weapon": true}],
            "inherits": {"namePrefix": "+1 ", "source": "XDMG", "rarity": "uncommon", "bonusWeapon": "+1",
                "entries": ["You have a {=bonusWeapon} bonus to attack and damage rolls made with this magic weapon."]}});
        let item = variant_as_item(&v);
        assert_eq!(item["source"], "XDMG");
        let md = item_card(&item, &refs());
        assert!(md.contains("**Kind:** Weapon (any)\n"), "{md}");
        assert!(md.ends_with("You have a +1 bonus to attack and damage rolls made with this magic weapon."));
    }

    #[test]
    fn placeholders_resolve_inside_tags() {
        let v = json!({"name": "Ammunition of Slaying", "requires": [{"type": "A"}],
            "inherits": {"source": "DMG", "rarity": "very rare",
                "entries": ["There are both {@i {=baseName/l}s of dragon slaying}. {=baseName/at} of slaying is rare."]}});
        let md = item_card(&variant_as_item(&v), &refs());
        assert!(md.ends_with("There are both *ammunitions of dragon slaying*. An ammunition of slaying is rare."), "{md}");
        let w = json!({"name": "Sword", "requires": [{"sword": true}], "dmgType": "S",
            "entries": ["{=baseName/a} deals {=dmgType} damage."]});
        assert_eq!(item_card(&w, &refs()).lines().last(), Some("a weapon deals slashing damage."));
    }

    #[test]
    fn lists_and_tables_become_lines() {
        let e = json!([
            {"type": "list", "items": ["One {@dice 1d4}", {"type": "item", "name": "Two", "entry": "second"}]},
            {"type": "table", "caption": "Effects", "colLabels": ["d6", "Effect"],
             "rows": [[{"type": "cell", "roll": {"exact": 1}}, "Sparks"], [{"type": "cell", "roll": {"min": 2, "max": 6}}, "Smoke"]]},
            {"type": "entries", "name": "Curse", "entries": ["Bad *luck* and_more", "-----"]}
        ]);
        assert_eq!(
            paragraphs(&e),
            vec![
                "• One 1d4",
                "• **Two.** second",
                "**Effects**",
                "**d6 — Effect**",
                "1 — Sparks",
                "2–6 — Smoke",
                "**Curse.** Bad *luck* andmore",
            ]
        );
    }
}
