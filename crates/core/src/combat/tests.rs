use serde_json::json;

use super::*;

fn ids() -> impl FnMut() -> String {
    let mut n = 0;
    move || {
        n += 1;
        format!("id{n:02}")
    }
}

fn row(id: &str, name: &str, init: i64, source: Option<&str>) -> Row {
    Row {
        id: id.into(),
        name: name.into(),
        initiative: Some(init),
        source: source.map(Into::into),
        ordinal: 1,
        is_player_visible: true,
        ..Row::default()
    }
}

fn col(id: &str, p: &str) -> Column {
    Column {
        id: id.into(),
        is_editable: true,
        is_player_visible: 0,
        populate_with: p.into(),
        abbreviation: String::new(),
    }
}

fn tracker() -> Tracker {
    Tracker {
        rows: vec![
            row("a", "Vex", 15, None),
            row("b", "Goblin", 12, Some("MM")),
            row("c", "Ogre", 8, Some("MM")),
        ],
        ..Tracker::default()
    }
}

fn active(t: &Tracker) -> Vec<&str> {
    t.rows.iter().filter(|r| r.is_active).map(|r| r.name.as_str()).collect()
}

#[test]
fn sorts_like_5etools() {
    let mut t = tracker();
    t.rows.push(row("d", "goblin", 12, Some("MM")));
    t.sort_rows();
    let names = |t: &Tracker| t.rows.iter().map(|r| r.id.clone()).collect::<Vec<_>>();
    // initiative desc, then name desc (case-insensitive), then ordinal asc, then id asc
    assert_eq!(names(&t), ["a", "b", "d", "c"]);
    t.set_sort(Sort::Alpha);
    assert_eq!((t.sort, t.dir), (Sort::Alpha, Dir::Asc));
    assert_eq!(names(&t), ["b", "d", "c", "a"]);
    t.set_sort(Sort::Alpha);
    assert_eq!(t.dir, Dir::Desc);
    t.set_sort(Sort::Number);
    assert_eq!((t.sort, t.dir), (Sort::Number, Dir::Desc));
}

#[test]
fn turns_wrap_and_count_rounds() {
    let mut t = tracker();
    let mut rng = |_| 0;
    t.next_turn(&mut rng, &|_| None);
    assert_eq!((active(&t), t.round), (vec!["Vex"], 1), "first Next activates the top row");
    t.next_turn(&mut rng, &|_| None);
    t.next_turn(&mut rng, &|_| None);
    assert_eq!((active(&t), t.round), (vec!["Ogre"], 1));
    t.next_turn(&mut rng, &|_| None);
    assert_eq!((active(&t), t.round), (vec!["Vex"], 2), "wrapping starts round 2");
    t.prev_turn();
    assert_eq!((active(&t), t.round), (vec!["Ogre"], 2), "going back never changes the round");
    t.prev_turn();
    assert_eq!(active(&t), vec!["Goblin"]);
}

#[test]
fn similar_rows_share_a_turn_at_the_same_initiative() {
    let mut t = tracker();
    let mut g2 = row("b2", "Goblin", 12, Some("MM"));
    g2.ordinal = 2;
    let mut g3 = row("b3", "Goblin", 5, Some("MM"));
    g3.ordinal = 3;
    t.rows.extend([g2, g3]);
    t.rows[0].is_active = true;
    t.next_turn(&mut |_| 0, &|_| None);
    let ids: Vec<_> = t.rows.iter().filter(|r| r.is_active).map(|r| r.id.as_str()).collect();
    assert_eq!(ids, ["b", "b2"], "the goblin at 5 waits for its own turn");
}

#[test]
fn conditions_tick_down_when_the_turn_ends() {
    let mut t = tracker();
    let mut new_id = ids();
    t.add_condition("a", "Prone", "#5e60a0", Some(1), &mut new_id);
    t.add_condition("a", "Blessed", "#fff", Some(2), &mut new_id);
    t.add_condition("a", "Concentration", "#009f7a", None, &mut new_id);
    t.add_condition("a", "Zero", "#000", Some(0), &mut new_id);
    t.ensure_active();
    t.next_turn(&mut |_| 0, &|_| None);
    let names: Vec<_> = t
        .rows
        .iter()
        .find(|r| r.id == "a")
        .unwrap()
        .conditions
        .iter()
        .map(|c| (c.name.as_str(), c.turns))
        .collect();
    assert_eq!(names, [("Blessed", Some(1)), ("Concentration", None), ("Zero", None)]);
}

#[test]
fn columns_reset_on_turn_and_round_start() {
    let mut t = tracker();
    t.stats_cols =
        vec![col("re", "cbAutoLow"), col("rd", "cbAutoRoundHigh"), col("la", "legendaryActions")];
    for r in &mut t.rows {
        r.cells = vec![
            Cell { id: "re".into(), entity: json!({"value": true}).as_object().unwrap().clone() },
            Cell { id: "rd".into(), entity: json!({"value": false}).as_object().unwrap().clone() },
            Cell {
                id: "la".into(),
                entity: json!({"current": 0, "max": 3}).as_object().unwrap().clone(),
            },
        ];
    }
    t.rows[2].is_active = true; // Ogre, last
    t.next_turn(&mut |_| 0, &|_| None); // → Vex: round 2 starts
    let cell = |t: &Tracker, r: usize, c: usize| t.rows[r].cells[c].entity.clone();
    assert_eq!(cell(&t, 0, 0)["value"], json!(false), "reaction used clears on Vex's turn");
    assert_eq!(cell(&t, 1, 0)["value"], json!(true), "others keep theirs until their turn");
    assert!(
        (0..3).all(|r| cell(&t, r, 1)["value"] == json!(true)),
        "round start ticks for everyone"
    );
    assert!((0..3).all(|r| cell(&t, r, 2)["current"] == json!(3)), "legendary actions refill");
    t.rows[1].cells[0].entity.insert("value".into(), json!(true));
    t.prev_turn(); // backwards leaves cells alone
    assert_eq!(cell(&t, 2, 0)["value"], json!(true));
}

#[test]
fn reroll_each_round_resorts_creatures() {
    let mut t = tracker();
    t.is_reroll_initiative_each_round = true;
    t.rows[2].is_active = true;
    // rerolled in order: the goblin rolls a 20 (+0), the ogre a 1; Vex has no bonus → keeps 15
    let mut rolls = vec![19u32, 0].into_iter();
    let bonus = |r: &Row| r.is_creature().then_some(0);
    t.next_turn(&mut |_| rolls.next().unwrap_or(0), &bonus);
    let order: Vec<_> = t.rows.iter().map(|r| (r.name.as_str(), r.initiative)).collect();
    assert_eq!(order, [("Goblin", Some(20)), ("Vex", Some(15)), ("Ogre", Some(1))]);
    assert_eq!((active(&t), t.round), (vec!["Goblin"], 2));
}

#[test]
fn wound_levels() {
    let w =
        |cur, max, inv| wound_level(&Row { hp_current: cur, hp_max: max, ..Row::default() }, inv);
    assert_eq!(w(Some(10), Some(10), false), WoundLevel::Healthy);
    assert_eq!(w(Some(6), Some(10), false), WoundLevel::Hurt);
    assert_eq!(w(Some(5), Some(10), false), WoundLevel::Bloodied);
    assert_eq!(w(Some(0), Some(10), false), WoundLevel::Defeated);
    assert_eq!(w(Some(-4), Some(10), false), WoundLevel::Defeated);
    assert_eq!(w(None, Some(10), false), WoundLevel::Unknown);
    assert_eq!(
        w(Some(0), Some(10), true),
        WoundLevel::Healthy,
        "as damage: 0 taken is full health"
    );
    assert_eq!(w(Some(10), Some(10), true), WoundLevel::Defeated);
}

#[test]
fn adding_creatures_rolls_and_numbers_them() {
    let goblin = json!({"name": "Goblin", "source": "MM", "hp": {"average": 7, "formula": "2d6"}, "dex": 14, "ac": [15]});
    let mut t = Tracker { stats_cols: vec![col("ac", "armorClass")], ..Tracker::default() };
    let mut rolls = vec![9u32, 0, 0, 4, 5, 5].into_iter();
    let mut rng = |_| rolls.next().unwrap_or(0);
    let mut new_id = ids();
    let added = t.add_creatures(&goblin, 2, true, None, "", &mut rng, &mut new_id);
    assert_eq!(added.len(), 2);
    // first copy: init 10+2, HP 1+1; second: init 5+2, HP 6+6 (not grouped)
    let g: Vec<_> =
        t.rows.iter().map(|r| (r.ordinal, r.initiative, r.hp_max, r.hp_current)).collect();
    assert_eq!(g, [(1, Some(12), Some(2), Some(2)), (2, Some(7), Some(12), Some(12))]);
    assert_eq!(t.rows[0].cells[0].entity["value"], json!(15));
    assert!(!t.rows[0].is_player_visible, "new monsters are hidden from players by default");
    assert!(t.shows_ordinal(&t.rows[0]));

    t.is_roll_groups = true;
    let added =
        t.add_creatures(&goblin, 3, false, Some("Boss".into()), "", &mut |_| 3, &mut new_id);
    let boss: Vec<_> = t
        .rows
        .iter()
        .filter(|r| added.contains(&r.id))
        .map(|r| (r.initiative, r.hp_max, r.ordinal))
        .collect();
    assert_eq!(boss, [(Some(6), Some(7), 1), (Some(6), Some(7), 2), (Some(6), Some(7), 3)]);
}

#[test]
fn duplicate_delete_and_reset() {
    let mut t = tracker();
    t.rows_default_party = vec![row("p", "Vex", 0, None)];
    let mut new_id = ids();
    let copy = t.duplicate("b", None, &mut |_| 0, &mut new_id).unwrap();
    let c = t.rows.iter().find(|r| r.id == copy).unwrap();
    assert_eq!((c.initiative, c.ordinal), (Some(12), 2));
    assert_eq!(t.delete("b", true), 2, "shift-delete removes every goblin");
    t.round = 5;
    t.reset(&mut new_id);
    assert_eq!((t.rows.len(), t.rows[0].name.as_str(), t.round), (1, "Vex", 1));
    assert_ne!(t.rows[0].id, "p");
}

#[test]
fn serial_round_trip_matches_5etools_keys() {
    let mut t = tracker();
    t.round = 3;
    t.notes = "lair".into();
    t.stats_cols = vec![Column {
        id: "x".into(),
        is_editable: false,
        is_player_visible: 2,
        populate_with: "armorClass".into(),
        abbreviation: "AC".into(),
    }];
    t.conditions_custom = vec![CustomCondition {
        id: "cc".into(),
        name: "Hexed".into(),
        color: "#800080".into(),
        turns: Some(10),
    }];
    t.rows[1].conditions = vec![Condition {
        id: "k".into(),
        name: "Prone".into(),
        color: "#5e60a0".into(),
        turns: None,
    }];
    t.rows[1].cells =
        vec![Cell { id: "x".into(), entity: json!({"value": 15}).as_object().unwrap().clone() }];
    t.rows[1].hp_current = Some(4);
    t.rows[1].hp_max = Some(7);
    t.extra.insert("future".into(), json!(1));
    let s = t.to_serial();
    assert_eq!(s["n"], json!(3));
    assert_eq!(s["c"][0], json!({"id": "x", "e": false, "v": 2, "p": "armorClass", "a": "AC"}));
    assert_eq!(s["cndc"][0], json!({"id": "cc", "n": "Hexed", "c": "#800080", "t": 10}));
    assert_eq!(
        s["r"][1],
        json!({"id": "b", "n": "Goblin", "h": 4, "g": 7, "i": 12, "a": false, "s": "MM",
               "c": [{"id": "k", "entity": {"name": "Prone", "color": "#5e60a0"}}],
               "nt": "", "v": true, "o": 1, "k": [{"id": "x", "v": 15}]})
    );
    assert_eq!(Tracker::from_serial(&s, &mut ids()), t);
}

#[test]
fn serial_reads_legacy_5etools_rows() {
    let legacy = json!({
        "r": [{"n": {"n": "Goblin", "m": "Snik", "scr": "2"}, "h": "5", "g": 7, "i": "14", "a": 1, "s": "MM",
               "c": [{"name": "Prone", "color": "#5e60a0", "turns": "2"}]}],
        "n": "x", "s": "ALPHA"
    });
    let t = Tracker::from_serial(&legacy, &mut ids());
    let r = &t.rows[0];
    assert_eq!(
        (r.name.as_str(), r.custom_name.as_deref(), r.scaled_cr),
        ("Goblin", Some("Snik"), Some(2.0))
    );
    assert_eq!((r.hp_current, r.initiative, r.is_active, r.ordinal), (Some(5), Some(14), true, 1));
    assert_eq!((r.conditions[0].name.as_str(), r.conditions[0].turns), ("Prone", Some(2)));
    assert_eq!((t.round, t.sort, t.dir), (1, Sort::Alpha, Dir::Desc));
    assert!(!r.id.is_empty());
}
