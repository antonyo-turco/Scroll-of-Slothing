//! Reading 5etools creature JSON (`data/bestiary/*.json`) the way the
//! Initiative Tracker does: initiative bonus, HP, the stat columns, dice.

use serde_json::{Value, json};

/// `VeCt.CR_CUSTOM`: CRs at or above it are not real challenge ratings.
pub const CR_CUSTOM: f64 = 100_000.0;

pub const ABILITIES: [&str; 6] = ["str", "dex", "con", "int", "wis", "cha"];

/// `Parser.SKILL_TO_ATB_ABV` with `Parser.SKILL_TO_SHORT`.
pub const SKILLS: [(&str, &str, &str); 18] = [
    ("acrobatics", "dex", "acro"),
    ("animal handling", "wis", "hndl"),
    ("arcana", "int", "arc"),
    ("athletics", "str", "ath"),
    ("deception", "cha", "decp"),
    ("history", "int", "hist"),
    ("insight", "wis", "ins"),
    ("intimidation", "cha", "intm"),
    ("investigation", "int", "invn"),
    ("medicine", "wis", "med"),
    ("nature", "int", "natr"),
    ("perception", "wis", "perp"),
    ("performance", "cha", "perf"),
    ("persuasion", "cha", "pers"),
    ("religion", "int", "reli"),
    ("sleight of hand", "dex", "soh"),
    ("stealth", "dex", "slth"),
    ("survival", "wis", "surv"),
];

/// `"animal handling"` → `"animalHandling"` (5etools `toCamelCase`).
pub fn camel(s: &str) -> String {
    let mut out = String::new();
    for (i, w) in s.split_whitespace().enumerate() {
        let mut c = w.chars();
        if let Some(f) = c.next() {
            if i == 0 {
                out.extend(f.to_lowercase());
            } else {
                out.extend(f.to_uppercase());
            }
            out.push_str(c.as_str());
        }
    }
    out
}

/// 5etools hash of a creature: `encodeURIComponent("name_source")` lower-cased.
pub fn creature_hash(name: &str, source: &str) -> String {
    let raw = format!("{name}_{source}").to_lowercase();
    let mut out = String::new();
    for b in raw.bytes() {
        let keep = b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b);
        if keep {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

pub fn ability_mod(score: i64) -> i64 {
    (score - 10).div_euclid(2)
}

/// `Parser.getAbilityModifier`: `"+2"`, `"-1"`.
pub fn mod_str(m: i64) -> String {
    if m >= 0 { format!("+{m}") } else { m.to_string() }
}

fn score(mon: &Value, att: &str) -> Option<i64> {
    mon.get(att).and_then(Value::as_i64)
}

fn cr_value(mon: &Value) -> Option<&Value> {
    mon.get("cr").map(|c| c.get("cr").unwrap_or(c))
}

/// `Parser.crToNumber` (None for unknown, `CR_CUSTOM` for unparsable).
pub fn cr_number(mon: &Value) -> Option<f64> {
    let s = cr_value(mon)?.as_str()?;
    if s == "Unknown" || s == "\u{2014}" {
        return None;
    }
    let parts: Vec<&str> = s.trim().split('/').filter(|p| !p.is_empty()).collect();
    let num = |p: &str| p.trim().parse::<f64>().ok();
    Some(match parts.as_slice() {
        [a] => num(a).unwrap_or(CR_CUSTOM),
        [a, b] => match (num(a), num(b)) {
            (Some(a), Some(b)) => a / b,
            _ => CR_CUSTOM,
        },
        _ => CR_CUSTOM,
    })
}

/// `Parser.crToPb`.
pub fn cr_to_pb(mon: &Value) -> Option<i64> {
    match cr_number(mon) {
        None => Some(0),
        Some(n) if n >= CR_CUSTOM || n < 0.0 => None,
        Some(n) if n < 5.0 => Some(2),
        Some(n) => Some((n / 4.0).ceil() as i64 + 1),
    }
}

const XP: [(&str, i64); 34] = [
    ("0", 10),
    ("1/8", 25),
    ("1/4", 50),
    ("1/2", 100),
    ("1", 200),
    ("2", 450),
    ("3", 700),
    ("4", 1100),
    ("5", 1800),
    ("6", 2300),
    ("7", 2900),
    ("8", 3900),
    ("9", 5000),
    ("10", 5900),
    ("11", 7200),
    ("12", 8400),
    ("13", 10000),
    ("14", 11500),
    ("15", 13000),
    ("16", 15000),
    ("17", 18000),
    ("18", 20000),
    ("19", 22000),
    ("20", 25000),
    ("21", 33000),
    ("22", 41000),
    ("23", 50000),
    ("24", 62000),
    ("25", 75000),
    ("26", 90000),
    ("27", 105000),
    ("28", 120000),
    ("29", 135000),
    ("30", 155000),
];

/// `Parser.crToXpNumber`.
pub fn cr_to_xp(mon: &Value) -> Option<i64> {
    if let Some(xp) = mon.get("cr").and_then(|c| c.get("xp")).and_then(Value::as_i64) {
        return Some(xp);
    }
    let s = cr_value(mon)?.as_str()?;
    XP.iter().find(|(c, _)| *c == s).map(|(_, x)| *x)
}

/// `Renderer.monster.getInitiativeBonusNumber`.
pub fn initiative_bonus(mon: &Value) -> Option<i64> {
    let dex = mon.get("dex");
    let dex_mod = dex.and_then(Value::as_i64).map(ability_mod);
    match mon.get("initiative") {
        None | Some(Value::Null) => dex_mod,
        Some(Value::Number(n)) => n.as_i64(),
        Some(Value::Object(o)) => {
            if let Some(i) = o.get("initiative").and_then(Value::as_i64) {
                return Some(i);
            }
            let prof = o.get("proficiency").and_then(Value::as_i64).unwrap_or(0);
            let pb = if prof != 0 && cr_number(mon).is_some_and(|n| n < CR_CUSTOM) {
                prof * cr_to_pb(mon).unwrap_or(0)
            } else {
                0
            };
            Some(dex_mod? + pb)
        }
        _ => None,
    }
}

pub fn hp_average(mon: &Value) -> Option<i64> {
    mon.get("hp")?.get("average")?.as_i64()
}

pub fn hp_formula(mon: &Value) -> Option<&str> {
    mon.get("hp")?.get("formula")?.as_str()
}

fn armor_class(mon: &Value) -> Value {
    match mon.get("ac").and_then(|a| a.get(0)) {
        Some(Value::Object(o)) => o.get("ac").cloned().unwrap_or(Value::Null),
        Some(v) => v.clone(),
        None => Value::Null,
    }
}

fn speed(mon: &Value) -> i64 {
    let Some(Value::Object(sp)) = mon.get("speed") else { return 0 };
    sp.values()
        .filter_map(|v| v.get("number").and_then(Value::as_i64).or_else(|| v.as_i64()))
        .fold(0, i64::max)
}

fn spell_dc(mon: &Value) -> i64 {
    let re = regex::Regex::new(r"DC (\d+)|\{@dc (\d+)\}").expect("valid regex");
    let mut best = 0;
    for sc in mon.get("spellcasting").and_then(Value::as_array).into_iter().flatten() {
        for e in sc.get("headerEntries").and_then(Value::as_array).into_iter().flatten() {
            if let Some(s) = e.as_str() {
                for c in re.captures_iter(s) {
                    let n = c.get(1).or(c.get(2)).and_then(|m| m.as_str().parse().ok());
                    best = best.max(n.unwrap_or(0));
                }
            }
        }
    }
    best
}

fn legendary_count(mon: &Value) -> Option<i64> {
    if let Some(n) = mon.get("legendaryActions").and_then(Value::as_i64) {
        return Some(n);
    }
    let has = mon.get("legendary").and_then(Value::as_array).is_some_and(|a| !a.is_empty());
    has.then_some(3)
}

/// `Renderer.monster.getTokenUrl`, absolute under the mirror's base URL.
pub fn token_url(mon: &Value, base_url: &str) -> Option<String> {
    if let Some(u) = mon.get("tokenUrl").and_then(Value::as_str) {
        return Some(u.to_string());
    }
    let name = mon.get("name")?.as_str()?.replace('"', "");
    let source = mon.get("source")?.as_str()?;
    Some(format!("{}/img/bestiary/tokens/{source}/{name}.webp", base_url.trim_end_matches('/')))
}

/// The cell a stat column gets when a creature (or, with `mon = None`, a
/// player) is added: the fields of the cell's `entity` (`value`, or
/// `current`/`max`, or `tokenUrl`/`imageHref`).
pub fn initial_cell(populate_with: &str, mon: Option<&Value>, base_url: &str) -> Value {
    let Some(mon) = mon else {
        return match populate_with {
            "legendaryActions" => json!({"current": null, "max": null}),
            "image" => json!({"tokenUrl": null, "imageHref": null}),
            "cbNeutral" | "cbAutoLow" | "cbAutoRoundLow" => json!({"value": false}),
            "cbAutoHigh" | "cbAutoRoundHigh" => json!({"value": true}),
            "" => json!({"value": ""}),
            _ => json!({"value": null}),
        };
    };
    let v = |x: Value| json!({ "value": x });
    match populate_with {
        "hpFormula" => v(json!(hp_formula(mon).unwrap_or(""))),
        "armorClass" => v(armor_class(mon)),
        "passivePerception" => v(mon.get("passive").cloned().unwrap_or(Value::Null)),
        "speed" => v(json!(speed(mon))),
        "spellDc" => v(json!(spell_dc(mon))),
        "initiative" => v(json!(initiative_bonus(mon))),
        "cr" => v(json!(cr_number(mon).filter(|n| *n < CR_CUSTOM))),
        "xp" => v(json!(cr_to_xp(mon))),
        "legendaryActions" => {
            let n = legendary_count(mon);
            json!({"current": n, "max": n})
        }
        "image" => json!({"tokenUrl": token_url(mon, base_url), "imageHref": null}),
        "cbNeutral" | "cbAutoLow" | "cbAutoRoundLow" => v(json!(false)),
        "cbAutoHigh" | "cbAutoRoundHigh" => v(json!(true)),
        "" => v(json!("")),
        p => {
            for att in ABILITIES {
                let m = || score(mon, att).map(|s| mod_str(ability_mod(s)));
                if p == format!("{att}Save") {
                    let save = mon.get("save").and_then(|s| s.get(att)).cloned();
                    return v(save.unwrap_or_else(|| json!(m())));
                }
                if p == format!("{att}Bonus") {
                    return v(json!(m()));
                }
                if p == format!("{att}Score") {
                    return v(json!(score(mon, att)));
                }
            }
            for (skill, att, _) in SKILLS {
                if p == camel(skill) {
                    let s = mon.get("skill").and_then(|s| s.get(skill)).cloned();
                    let m = score(mon, att).map(|s| mod_str(ability_mod(s)));
                    return v(s.unwrap_or_else(|| json!(m)));
                }
            }
            v(Value::Null)
        }
    }
}

/// A stat column type: `populateWith` id, default abbreviation and name.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ColumnKind {
    pub populate_with: String,
    pub abbreviation: String,
    pub name: String,
}

/// Every column type, in the 5etools menu order (custom first).
pub fn column_kinds() -> Vec<ColumnKind> {
    let k = |p: &str, a: &str, n: &str| ColumnKind {
        populate_with: p.into(),
        abbreviation: a.into(),
        name: n.into(),
    };
    let full = |a: &str| match a {
        "str" => "Strength",
        "dex" => "Dexterity",
        "con" => "Constitution",
        "int" => "Intelligence",
        "wis" => "Wisdom",
        _ => "Charisma",
    };
    let mut out = vec![
        k("", "", "(Custom)"),
        k("hpFormula", "", "HP Formula"),
        k("armorClass", "AC", "Armor Class"),
        k("passivePerception", "PP", "Passive Perception"),
        k("speed", "SPD", "Speed"),
        k("spellDc", "DC", "Spell DC"),
        k("initiative", "INIT", "Initiative"),
        k("cr", "CR", "Challenge Rating (CR)"),
        k("xp", "XP", "Experience Points (XP)"),
        k("legendaryActions", "LA", "Legendary Actions"),
        k("image", "IMG", "Image"),
    ];
    for suffix in ["Save", "Bonus", "Score"] {
        for a in ABILITIES {
            out.push(k(
                &format!("{a}{suffix}"),
                &a.to_uppercase(),
                &format!("{} {suffix}", full(a)),
            ));
        }
    }
    for (skill, _, short) in SKILLS {
        let mut name = skill.to_string();
        name[..1].make_ascii_uppercase();
        out.push(k(&camel(skill), &short.to_uppercase(), &name));
    }
    out.extend([
        k("cbNeutral", "", "Checkbox"),
        k("cbAutoLow", "", "Checkbox; clears at start of turn"),
        k("cbAutoHigh", "", "Checkbox; ticks at start of turn"),
        k("cbAutoRoundLow", "", "Checkbox; clears at start of round"),
        k("cbAutoRoundHigh", "", "Checkbox; ticks at start of round"),
    ]);
    out
}

/// Rolls `XdY+Z`-style formulas (`2d6 + 4`, `1d20-1`, `12`). `rng(n)` returns
/// a number in `0..n`. None when the formula has other syntax.
pub fn roll(formula: &str, rng: &mut dyn FnMut(u32) -> u32) -> Option<i64> {
    let f: String = formula.chars().filter(|c| !c.is_whitespace()).collect();
    if f.is_empty() {
        return None;
    }
    let mut total = 0i64;
    let mut sign = 1i64;
    let mut term = String::new();
    let mut flush = |term: &mut String, sign: i64, total: &mut i64| -> Option<()> {
        if term.is_empty() {
            return None;
        }
        let v = if let Some((n, d)) = term.split_once(['d', 'D']) {
            let n: u32 = if n.is_empty() { 1 } else { n.parse().ok()? };
            let d: u32 = d.parse().ok()?;
            if d == 0 || n > 1000 {
                return None;
            }
            (0..n).map(|_| i64::from(rng(d)) + 1).sum()
        } else {
            term.parse::<i64>().ok()?
        };
        *total += sign * v;
        term.clear();
        Some(())
    };
    for (i, c) in f.chars().enumerate() {
        match c {
            '+' | '-' if i > 0 => {
                flush(&mut term, sign, &mut total)?;
                sign = if c == '-' { -1 } else { 1 };
            }
            '-' => sign = -1,
            '+' => {}
            _ => term.push(c),
        }
    }
    flush(&mut term, sign, &mut total)?;
    Some(total)
}

/// Initiative for a creature: `1d20 + bonus` (5etools `pGetRollInitiative`).
pub fn roll_initiative(bonus: i64, rng: &mut dyn FnMut(u32) -> u32) -> i64 {
    i64::from(rng(20)) + 1 + bonus
}

/// HP as 5etools `pGetOrRollHp`: the average unless rolling, else the formula.
pub fn hp_for(mon: &Value, roll_hp: bool, rng: &mut dyn FnMut(u32) -> u32) -> Option<i64> {
    if !roll_hp && let Some(a) = hp_average(mon) {
        return Some(a);
    }
    if roll_hp && let Some(f) = hp_formula(mon) {
        return roll(f, rng);
    }
    None
}

/// Resolves a `_copy` creature against its base: the base's fields, then the
/// copy's own (except `_copy`). `_mod` is not applied (see the feature doc).
pub fn resolve_copy(copy: &Value, base: &Value) -> Value {
    let mut out = base.as_object().cloned().unwrap_or_default();
    for k in ["_copy", "_mod", "_isCopy"] {
        out.remove(k);
    }
    if let Some(own) = copy.as_object() {
        for (k, v) in own {
            if k != "_copy" && k != "_mod" {
                out.insert(k.clone(), v.clone());
            }
        }
    }
    Value::Object(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn goblin() -> Value {
        json!({
            "name": "Goblin", "source": "MM", "hp": {"average": 7, "formula": "2d6"},
            "ac": [{"ac": 15, "from": ["leather armor"]}], "speed": {"walk": 30, "climb": {"number": 40}},
            "str": 8, "dex": 14, "con": 10, "int": 10, "wis": 8, "cha": 8,
            "skill": {"stealth": "+6"}, "passive": 9, "cr": "1/4"
        })
    }

    #[test]
    fn stats_follow_the_tracker_columns() {
        let g = goblin();
        let cell = |p| initial_cell(p, Some(&g), "http://localhost:8080/");
        assert_eq!(cell("armorClass"), json!({"value": 15}));
        assert_eq!(cell("speed"), json!({"value": 40}));
        assert_eq!(cell("passivePerception"), json!({"value": 9}));
        assert_eq!(cell("cr"), json!({"value": 0.25}));
        assert_eq!(cell("xp"), json!({"value": 50}));
        assert_eq!(cell("hpFormula"), json!({"value": "2d6"}));
        assert_eq!(cell("dexBonus"), json!({"value": "+2"}));
        assert_eq!(cell("wisSave"), json!({"value": "-1"}));
        assert_eq!(cell("strScore"), json!({"value": 8}));
        assert_eq!(cell("stealth"), json!({"value": "+6"}));
        assert_eq!(cell("animalHandling"), json!({"value": "-1"}));
        assert_eq!(cell("legendaryActions"), json!({"current": null, "max": null}));
        assert_eq!(
            cell("image"),
            json!({"tokenUrl": "http://localhost:8080/img/bestiary/tokens/MM/Goblin.webp", "imageHref": null})
        );
        assert_eq!(initial_cell("cbAutoHigh", None, ""), json!({"value": true}));
    }

    #[test]
    fn initiative_bonus_variants() {
        assert_eq!(initiative_bonus(&goblin()), Some(2));
        assert_eq!(initiative_bonus(&json!({"initiative": 7, "dex": 10})), Some(7));
        // proficiency × PB(CR 5 = 3)
        let m = json!({"dex": 14, "cr": "5", "initiative": {"proficiency": 2}});
        assert_eq!(initiative_bonus(&m), Some(8));
        assert_eq!(initiative_bonus(&json!({"name": "x"})), None);
    }

    #[test]
    fn cr_and_spell_dc_and_legendary() {
        assert_eq!(cr_number(&json!({"cr": {"cr": "1/2", "lair": "1"}})), Some(0.5));
        assert_eq!(cr_to_pb(&json!({"cr": "17"})), Some(6));
        assert_eq!(cr_to_xp(&json!({"cr": {"cr": "5", "xp": 2000}})), Some(2000));
        let lich = json!({
            "spellcasting": [{"headerEntries": ["spell save {@dc 20}, DC 18 to hit"]}],
            "legendary": [{"name": "Cantrip"}]
        });
        assert_eq!(initial_cell("spellDc", Some(&lich), ""), json!({"value": 20}));
        assert_eq!(
            initial_cell("legendaryActions", Some(&lich), ""),
            json!({"current": 3, "max": 3})
        );
    }

    #[test]
    fn dice_and_hp() {
        let mut max = |n: u32| n - 1;
        assert_eq!(roll("2d6 + 4", &mut max), Some(16));
        assert_eq!(roll("1d20-1", &mut max), Some(19));
        assert_eq!(roll("-3+d4", &mut max), Some(1));
        assert_eq!(roll("12", &mut max), Some(12));
        assert_eq!(roll("2d6 × 3", &mut max), None);
        assert_eq!(roll("", &mut max), None);
        let mut min = |_: u32| 0;
        assert_eq!(roll_initiative(2, &mut min), 3);
        assert_eq!(hp_for(&goblin(), false, &mut min), Some(7));
        assert_eq!(hp_for(&goblin(), true, &mut min), Some(2));
    }

    #[test]
    fn hashes_and_copies() {
        assert_eq!(creature_hash("Goblin", "MM"), "goblin_mm");
        assert_eq!(creature_hash("Adult Red Dragon", "MM"), "adult%20red%20dragon_mm");
        assert_eq!(camel("sleight of hand"), "sleightOfHand");
        let base = goblin();
        let copy =
            json!({"name": "Goblin Boss", "source": "X", "_copy": {"name": "Goblin"}, "cr": "1"});
        let r = resolve_copy(&copy, &base);
        assert_eq!(
            (r["name"].as_str(), r["cr"].as_str(), r["dex"].as_i64()),
            (Some("Goblin Boss"), Some("1"), Some(14))
        );
        assert!(r.get("_copy").is_none());
        assert_eq!(column_kinds().len(), 11 + 18 + 18 + 5);
    }
}
