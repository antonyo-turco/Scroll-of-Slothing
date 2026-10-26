//! Music track categories and tag suggestions. See `docs/features/music/library-tracks.md`.

/// What a track is for. It decides where "Add to mix" puts it.
pub const CATEGORIES: [&str; 3] = ["music", "ambient", "soundbite"];

pub fn is_category(c: &str) -> bool {
    CATEGORIES.contains(&c)
}

/// The mixer channel role for a category (soundbites go to the soundbite list
/// in the UI; as a plain channel they play as one-shot `sfx`).
pub fn role_for_category(category: &str) -> &'static str {
    match category {
        "ambient" => "ambient",
        "soundbite" => "sfx",
        _ => "music",
    }
}

/// Short one-shots up to this length (seconds) count as soundbites.
pub const SOUNDBITE_MAX_S: f64 = 120.0;

fn words(title: &str) -> String {
    format!(" {} ", title.to_lowercase().replace(|c: char| !c.is_alphanumeric(), " "))
}

fn has_any(w: &str, keys: &[&str]) -> bool {
    keys.iter().any(|k| w.contains(&format!(" {k}")))
}

/// Best guess for a new or legacy track, from its title, loop flag and length.
/// The user can always change it.
pub fn guess_category(title: &str, loop_by_default: bool, duration: Option<f64>) -> &'static str {
    let w = words(title);
    if has_any(&w, &["sfx", "sound effect", "soundbite", "one shot", "oneshot"]) {
        return "soundbite";
    }
    if !loop_by_default && duration.is_some_and(|d| d <= SOUNDBITE_MAX_S) {
        return "soundbite";
    }
    // "music" in the title wins over setting words such as "swamp village".
    if has_any(&w, &["music", "theme", "soundtrack", "ost", "song", "battle music", "combat music"])
    {
        return "music";
    }
    if has_any(
        &w,
        &["ambienc", "ambient", "atmospher", "soundscape", "noise", "sounds", "dripping"],
    ) {
        return "ambient";
    }
    "music"
}

/// Keyword → tag. A tag is suggested when any of its keywords starts a word in
/// the title (so "battle" also matches "battles", "rat" matches "rats").
const TAG_RULES: &[(&str, &[&str])] = &[
    // places
    ("swamp", &["swamp", "bog", "marsh", "fen"]),
    ("forest", &["forest", "woods", "woodland", "trees"]),
    ("jungle", &["jungle"]),
    ("cave", &["cave", "cavern", "warren"]),
    ("dungeon", &["dungeon", "catacomb", "crypt", "ruins", "ruin"]),
    ("temple", &["temple", "sanctuary", "shrine"]),
    ("sewer", &["sewer"]),
    ("village", &["village", "town"]),
    ("tavern", &["tavern", "inn", "pub"]),
    ("city", &["city", "market"]),
    ("sea", &["sea", "ocean", "cove", "ship", "harbor", "harbour"]),
    ("void", &["void"]),
    // moods and scenes
    ("combat", &["battle", "combat", "fight", "boss"]),
    ("exploration", &["exploration", "explore", "travel"]),
    ("peaceful", &["peaceful", "calm", "relax"]),
    ("mysterious", &["mysterious", "enigmatic", "mystery"]),
    ("dark", &["dark", "evil", "creepy", "horror", "darkest"]),
    ("busy", &["busy", "crowd"]),
    // sounds and creatures
    ("nature", &["nature", "birds", "creek", "river"]),
    ("water", &["water", "dripping", "creek", "river", "rain"]),
    ("insects", &["insect", "hive", "bug", "swarm", "crawling"]),
    // Whole words only ("rat " ends at a space): "rattling" is not a rat.
    ("rats", &["rat ", "rats", "ratkin", "wererat"]),
    ("undead", &["undead", "skeleton", "zombie", "ghost", "lich"]),
];

/// Games and soundtracks named in titles. Each gives its own tag, and its name
/// is removed before the other rules run, so "Darkest Dungeon" alone doesn't
/// also mean "dark" and "dungeon".
const SOURCE_RULES: &[(&str, &[&str])] = &[
    ("darkest dungeon", &["darkest dungeon"]),
    ("witcher", &["witcher"]),
    ("zelda", &["zelda"]),
    ("hollow knight", &["hollow knight"]),
];

/// Tags suggested by the title, in rule order (sources last), without duplicates.
pub fn suggest_tags(title: &str) -> Vec<&'static str> {
    let mut w = words(title);
    let mut sources: Vec<&'static str> = Vec::new();
    for (tag, keys) in SOURCE_RULES {
        for k in keys.iter() {
            let needle = format!(" {k}");
            if w.contains(&needle) {
                w = w.replace(&needle, " ");
                if !sources.contains(tag) {
                    sources.push(tag);
                }
            }
        }
    }
    let mut out: Vec<&'static str> = Vec::new();
    for (tag, keys) in TAG_RULES {
        if has_any(&w, keys) && !out.contains(tag) {
            out.push(tag);
        }
    }
    out.extend(sources.into_iter().filter(|s| !out.contains(s)).collect::<Vec<_>>());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categories_from_real_titles() {
        let g = |t: &str, l: bool, d: f64| guess_category(t, l, Some(d));
        assert_eq!(g("SFX - Rat Screech", false, 16.0), "soundbite");
        assert_eq!(g("Door slam", false, 3.0), "soundbite");
        assert_eq!(g("Peaceful Forest Ambience (birds, trees, creek)", true, 3601.0), "ambient");
        assert_eq!(g("Water Dripping in Cave (12h)", true, 43200.0), "ambient");
        assert_eq!(
            g("Swamp Village | Exploration Music (Wizard's Cup, 1h)", true, 3575.0),
            "music"
        );
        assert_eq!(g("Darkest Dungeon - Battle in the Warrens (1h)", true, 3661.0), "music");
        assert_eq!(g("Witcher 3 - Velen Nature Theme (1h)", true, 3631.0), "music");
        // A long non-looping track is not a soundbite.
        assert_eq!(g("Boss Fight", false, 300.0), "music");
        assert_eq!(guess_category("Unknown", false, None), "music");
    }

    #[test]
    fn tags_from_real_titles() {
        assert_eq!(
            suggest_tags("Dark Swamp | Exploration Ambience (Ghelfi, 1h)"),
            ["swamp", "exploration", "dark"]
        );
        // The game's name gives its own tag only, not "dark" or "dungeon".
        assert_eq!(
            suggest_tags("Darkest Dungeon - Battle in the Warrens (1h)"),
            ["cave", "combat", "darkest dungeon"]
        );
        assert_eq!(
            suggest_tags("Darkest Dungeon - The Cove Battle (1h)"),
            ["sea", "combat", "darkest dungeon"]
        );
        assert_eq!(suggest_tags("Zelda OoT - Forest Temple (1h)"), ["forest", "temple", "zelda"]);
        // "rattling" is not a rat.
        assert_eq!(suggest_tags("SFX - Skeleton Rattling"), ["undead"]);
        assert_eq!(
            suggest_tags("Filthy Sewers Ambience (water, rats)"),
            ["sewer", "water", "rats"]
        );
        assert_eq!(suggest_tags("SFX - Rats Squeaking & Squealing"), ["rats"]);
        assert_eq!(suggest_tags("Undead Crypt Ambience (skeletons)"), ["dungeon", "undead"]);
        // Keywords match word starts only: "ratkin" is a rat, "pirate" is not.
        assert_eq!(suggest_tags("Ratkin warband"), ["rats"]);
        assert!(suggest_tags("Pirate shanty").is_empty());
    }

    #[test]
    fn roles() {
        assert_eq!(role_for_category("ambient"), "ambient");
        assert_eq!(role_for_category("soundbite"), "sfx");
        assert_eq!(role_for_category("music"), "music");
        assert!(is_category("soundbite") && !is_category("sfx"));
    }
}
