//! A pure dice roller: parse standard notation (`2d6+3`, `d20`, `2d8+1d6+4`,
//! `d%`, `adv+5`) and roll it against a seeded PRNG. Deterministic given the
//! seed, no IO and no clock, so every rule is unit/property testable.
//! Spec: docs/features/dice-roller.md.
//!
//! The accepted syntax is a superset of `app/src/lib/dice.ts`'s `rollFormula`
//! (`NdM` and number terms joined by `+`/`-`, at most 200 dice). Additions:
//! `d%` (percentile, an alias for `d100`), and `adv` / `dis` (roll 2d20, keep
//! the higher / lower).

use std::fmt;

use fastrand::Rng;
use serde::Serialize;

/// Hard cap on dice in a single expression, matching `dice.ts` (`count > 200`).
pub const MAX_DICE: u32 = 200;
/// Largest die accepted (a d1000 is already silly; this only stops overflow games).
pub const MAX_SIDES: u32 = 10_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// Nothing rollable, e.g. empty, or only operators.
    Empty,
    /// A term that isn't `NdM`, `adv`, `dis` or a plain integer.
    Syntax(String),
    /// More than `MAX_DICE` dice in one expression.
    TooManyDice,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::Empty => write!(f, "nothing to roll"),
            ParseError::Syntax(t) => write!(f, "'{t}' isn't a die roll or a number"),
            ParseError::TooManyDice => write!(f, "at most {MAX_DICE} dice in one roll"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Term {
    /// `sign * NdM`, sign is +1 or -1.
    Dice { sign: i64, count: u32, sides: u32 },
    /// `sign * (2d20 keep higher/lower)`.
    Keep { sign: i64, high: bool },
    /// A plain integer term.
    Const { sign: i64, value: u32 },
}

/// A parsed expression: a canonical string plus its signed terms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expression {
    /// Canonical form, e.g. `2d6 + 3`, `d20`, `d100`, `adv − 1`. Parses back to itself.
    pub text: String,
    terms: Vec<Term>,
}

impl Expression {
    /// Parse a dice expression. Whitespace and case are ignored; `−` is a minus.
    pub fn parse(input: &str) -> Result<Self, ParseError> {
        let clean: String = input
            .to_lowercase()
            .replace('−', "-")
            .replace("d%", "d100")
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        let chunks = split_signed(&clean);
        if chunks.iter().all(|c| sign_of(c).1.is_empty()) {
            return Err(ParseError::Empty);
        }
        let mut terms = Vec::new();
        let mut dice = 0u32;
        for chunk in &chunks {
            let (sign, body) = sign_of(chunk);
            let term = match body {
                "adv" | "dis" => Term::Keep { sign, high: body == "adv" },
                _ => {
                    if let Some((count, sides)) = parse_die(body) {
                        Term::Dice { sign, count, sides }
                    } else if let Ok(value) = body.parse::<u32>() {
                        Term::Const { sign, value }
                    } else {
                        return Err(ParseError::Syntax(body.to_string()));
                    }
                }
            };
            dice = dice.saturating_add(match term {
                Term::Dice { count, .. } => count,
                Term::Keep { .. } => 2,
                Term::Const { .. } => 0,
            });
            if dice > MAX_DICE {
                return Err(ParseError::TooManyDice);
            }
            terms.push(term);
        }
        Ok(Expression { text: canonical(&terms), terms })
    }

    /// Lowest and highest possible totals.
    pub fn bounds(&self) -> (i64, i64) {
        self.terms.iter().fold((0, 0), |(lo, hi), t| {
            let (a, b) = match *t {
                Term::Dice { sign, count, sides } => {
                    (sign * i64::from(count), sign * i64::from(count) * i64::from(sides))
                }
                Term::Keep { sign, .. } => (sign, sign * 20),
                Term::Const { sign, value } => (sign * i64::from(value), sign * i64::from(value)),
            };
            (lo + a.min(b), hi + a.max(b))
        })
    }
}

/// One rolled dice term.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DieGroup {
    /// A `d20` has 20 sides.
    pub sides: u32,
    /// Each die's face, in roll order.
    pub faces: Vec<u32>,
    /// For `adv` / `dis`: the face that counts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kept: Option<u32>,
    /// -1 when the term is subtracted.
    pub sign: i64,
}

/// The result of a roll.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Roll {
    /// The canonical expression that was rolled (e.g. `2d6 + 3`).
    pub expression: String,
    pub total: i64,
    /// A human reading like `[3, 5] + 3 = 11` or `[14, 7] adv + 2 = 16`.
    pub detail: String,
    /// The dice terms, in order (constants are not listed).
    pub groups: Vec<DieGroup>,
}

/// Roll an expression with a seeded PRNG. The same seed always gives the same roll.
pub fn roll_expr(expr: &Expression, seed: u64) -> Roll {
    let mut rng = Rng::with_seed(seed);
    let mut groups = Vec::new();
    let mut parts: Vec<(i64, String)> = Vec::new();
    let mut total = 0i64;
    let mut any_dice = false;
    for t in &expr.terms {
        match *t {
            Term::Dice { sign, count, sides } => {
                any_dice = true;
                let faces: Vec<u32> = (0..count).map(|_| rng.u32(1..=sides)).collect();
                total += sign * faces.iter().map(|&f| i64::from(f)).sum::<i64>();
                parts.push((sign, faces_text(&faces)));
                groups.push(DieGroup { sides, faces, kept: None, sign });
            }
            Term::Keep { sign, high } => {
                any_dice = true;
                let faces = vec![rng.u32(1..=20), rng.u32(1..=20)];
                let kept = if high { faces[0].max(faces[1]) } else { faces[0].min(faces[1]) };
                total += sign * i64::from(kept);
                parts.push((
                    sign,
                    format!("{} {}", faces_text(&faces), if high { "adv" } else { "dis" }),
                ));
                groups.push(DieGroup { sides: 20, faces, kept: Some(kept), sign });
            }
            Term::Const { sign, value } => {
                total += sign * i64::from(value);
                parts.push((sign, value.to_string()));
            }
        }
    }
    let mut detail = join_signed(&parts);
    if any_dice {
        detail.push_str(&format!(" = {total}"));
    }
    Roll { expression: expr.text.clone(), total, detail, groups }
}

fn faces_text(faces: &[u32]) -> String {
    let list: Vec<String> = faces.iter().map(u32::to_string).collect();
    format!("[{}]", list.join(", "))
}

/// `[(1,"a"), (-1,"b"), (1,"c")]` → `a − b + c` (a leading minus hugs its term).
fn join_signed(parts: &[(i64, String)]) -> String {
    let mut out = String::new();
    for (i, (sign, body)) in parts.iter().enumerate() {
        match (i, *sign < 0) {
            (0, false) => out.push_str(body),
            (0, true) => out.push_str(&format!("−{body}")),
            (_, neg) => out.push_str(&format!(" {} {body}", if neg { "−" } else { "+" })),
        }
    }
    out
}

fn canonical(terms: &[Term]) -> String {
    let parts: Vec<(i64, String)> = terms
        .iter()
        .map(|t| match *t {
            Term::Dice { sign, count, sides } => {
                (sign, if count == 1 { format!("d{sides}") } else { format!("{count}d{sides}") })
            }
            Term::Keep { sign, high } => (sign, if high { "adv" } else { "dis" }.to_string()),
            Term::Const { sign, value } => (sign, value.to_string()),
        })
        .collect();
    join_signed(&parts)
}

// ── parser helpers ──────────────────────────────────────────────────────────

/// Split `a+b-c` into `["a", "+b", "-c"]` (the first element keeps its own sign if any).
fn split_signed(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for c in s.chars() {
        if (c == '+' || c == '-') && !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
        cur.push(c);
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn sign_of(chunk: &str) -> (i64, &str) {
    if let Some(rest) = chunk.strip_prefix('-') {
        (-1, rest)
    } else {
        (1, chunk.strip_prefix('+').unwrap_or(chunk))
    }
}

/// Parse a `NdM` body (count defaults to 1). Returns `(count, sides)`.
fn parse_die(body: &str) -> Option<(u32, u32)> {
    let (count, sides) = body.split_once('d')?;
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let count = if count.is_empty() {
        1
    } else if digits(count) {
        count.parse::<u32>().ok()?
    } else {
        return None;
    };
    if !digits(sides) {
        return None;
    }
    let sides = sides.parse::<u32>().ok()?;
    if count == 0 || sides == 0 || sides > MAX_SIDES {
        return None;
    }
    Some((count, sides))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn parses_common_forms() {
        let text = |s: &str| Expression::parse(s).unwrap().text;
        assert_eq!(text("2d6+3"), "2d6 + 3");
        assert_eq!(text("d20"), "d20");
        assert_eq!(text("1D20"), "d20");
        assert_eq!(text("d%"), "d100");
        assert_eq!(text("2D%+1"), "2d100 + 1");
        assert_eq!(text(" 2d6 + 2d4 + 4 "), "2d6 + 2d4 + 4");
        assert_eq!(text("1d20-1"), "d20 − 1");
        assert_eq!(text("-1+d4"), "−1 + d4");
        assert_eq!(text("ADV+5"), "adv + 5");
        assert_eq!(text("dis-2"), "dis − 2");
    }

    #[test]
    fn canonical_text_parses_back_to_itself() {
        for s in ["2d6+3", "d20-1", "-1+d4", "adv+5", "2d8+1d6-4", "d%"] {
            let once = Expression::parse(s).unwrap();
            assert_eq!(Expression::parse(&once.text).unwrap(), once, "{s}");
        }
    }

    #[test]
    fn rejects_bad_input() {
        assert_eq!(Expression::parse(""), Err(ParseError::Empty));
        assert_eq!(Expression::parse("   "), Err(ParseError::Empty));
        assert_eq!(Expression::parse("+"), Err(ParseError::Empty));
        for bad in ["abc", "d", "2d0", "0d6", "2d", "d-4", "2.5d6", "2d6+", "d20++3", "advx"] {
            assert!(matches!(Expression::parse(bad), Err(ParseError::Syntax(_))), "{bad}");
        }
        assert_eq!(Expression::parse("300d6"), Err(ParseError::TooManyDice));
        assert_eq!(Expression::parse("4d1+197d1"), Err(ParseError::TooManyDice));
        assert!(Expression::parse("199d1+1d1").is_ok());
    }

    #[test]
    fn roll_is_deterministic_for_a_seed() {
        let e = Expression::parse("2d6+3").unwrap();
        let a = roll_expr(&e, 42);
        assert_eq!(a, roll_expr(&e, 42));
        assert_eq!(a.groups.len(), 1);
        assert_eq!(a.groups[0].faces.len(), 2);
        let faces: i64 = a.groups[0].faces.iter().map(|&f| i64::from(f)).sum();
        assert_eq!(a.total, faces + 3);
        let (x, y) = (a.groups[0].faces[0], a.groups[0].faces[1]);
        assert_eq!(a.detail, format!("[{x}, {y}] + 3 = {}", a.total));
    }

    #[test]
    fn negative_terms_render_and_subtract() {
        let e = Expression::parse("d20-1d4-2").unwrap();
        let r = roll_expr(&e, 9);
        let (a, b) = (r.groups[0].faces[0], r.groups[1].faces[0]);
        assert_eq!(r.groups[1].sign, -1);
        assert_eq!(r.total, i64::from(a) - i64::from(b) - 2);
        assert_eq!(r.detail, format!("[{a}] − [{b}] − 2 = {}", r.total));
    }

    #[test]
    fn advantage_keeps_the_higher_and_disadvantage_the_lower() {
        for seed in 0..50 {
            let adv = roll_expr(&Expression::parse("adv+2").unwrap(), seed);
            let g = &adv.groups[0];
            assert_eq!(g.kept, Some(g.faces[0].max(g.faces[1])));
            assert_eq!(adv.total, i64::from(g.kept.unwrap()) + 2);
            assert!(adv.detail.contains(" adv + 2 = "));
            let dis = roll_expr(&Expression::parse("dis").unwrap(), seed);
            let g = &dis.groups[0];
            assert_eq!(g.kept, Some(g.faces[0].min(g.faces[1])));
        }
    }

    #[test]
    fn constant_only_has_no_equals() {
        let r = roll_expr(&Expression::parse("5").unwrap(), 1);
        assert_eq!((r.total, r.detail.as_str()), (5, "5"));
        assert!(r.groups.is_empty());
    }

    proptest! {
        #[test]
        fn totals_stay_in_bounds(
            count in 1u32..20, sides in 1u32..120, k in 0u32..30, neg: bool, adv: bool, seed: u64,
        ) {
            let op = if neg { '-' } else { '+' };
            let keep = if adv { "+adv" } else { "" };
            let e = Expression::parse(&format!("{count}d{sides}{op}{k}{keep}")).unwrap();
            let r = roll_expr(&e, seed);
            let (lo, hi) = e.bounds();
            prop_assert!(lo <= r.total && r.total <= hi);
            for g in &r.groups {
                prop_assert!(g.faces.iter().all(|&f| (1..=g.sides).contains(&f)));
            }
        }
    }
}
