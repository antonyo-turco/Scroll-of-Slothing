//! Card decks: markdown parser and layout engine.
//! Python reference: `card_generator.py`. Spec: docs/features/cards.md.
//!
//! The layout produces a list of drawing operations per page (`Op`). The PDF
//! writer and the UI preview both render these ops, so the preview is exact.
//! Coordinates are PDF points with a bottom-left origin, like ReportLab.

use std::sync::OnceLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::fonts::{FontStyle, width};
use crate::units::PageFormat;

pub const CARD_W: f64 = 180.0;
pub const CARD_H: f64 = 252.0;
pub const COLS: usize = 3;
pub const ROWS: usize = 3;
pub const PER_PAGE: usize = COLS * ROWS;
pub const BODY_FONT_SIZES: [f64; 5] = [13.0, 12.0, 11.0, 10.0, 9.0];
pub const STAT_FONT: f64 = 8.5;
pub const STAT_LINE_H: f64 = STAT_FONT * 1.45;
const PAD: f64 = 6.0;
const BANNER_H: f64 = 18.0;
const LINE_W: f64 = CARD_W - 2.0 * PAD;

pub type Rgb = [f64; 3];

pub fn type_color(kind: &str) -> Option<Rgb> {
    Some(match kind {
        "spell" => [0.5, 0.2, 0.7],
        "feat" => [0.2, 0.6, 0.3],
        "item" => [0.8, 0.5, 0.1],
        "ability" => [0.2, 0.4, 0.8],
        _ => return None,
    })
}
pub const DEFAULT_COLOR: Rgb = [0.4, 0.4, 0.4];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Card {
    pub title: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub stats: Vec<(String, String)>,
    pub body: String,
    pub color: Rgb,
    pub copies: u32,
}

fn re(cell: &'static OnceLock<Regex>, pat: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pat).expect("valid regex"))
}

/// Python `str.strip()` (strips Unicode whitespace).
fn py_strip(s: &str) -> &str {
    s.trim()
}

/// Python `str.splitlines()` (without keepends) for the common line breaks.
fn splitlines(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\n' => {
                out.push(&s[start..i]);
                start = i + 1;
            }
            b'\r' => {
                out.push(&s[start..i]);
                if bytes.get(i + 1) == Some(&b'\n') {
                    i += 1;
                }
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    if start < s.len() {
        out.push(&s[start..]);
    }
    out
}

/// Python `int()` on a stat value: optional surrounding whitespace and sign.
fn py_int(s: &str) -> Option<i64> {
    let t = s.trim().replace('_', "");
    if t.is_empty() || s.trim().starts_with('_') || s.trim().ends_with('_') {
        return None;
    }
    t.parse::<i64>().ok()
}

/// Parses a markdown deck (Python `parse_markdown`, same rules).
pub fn parse(text: &str) -> Vec<Card> {
    static SEP: OnceLock<Regex> = OnceLock::new();
    static STAT: OnceLock<Regex> = OnceLock::new();
    static HEX: OnceLock<Regex> = OnceLock::new();
    let sep = re(&SEP, r"\n---+\n");
    let stat_re = re(&STAT, r"^\*\*(.+?):\*\*\s*(.*)");
    let hex_re = re(&HEX, r"#([0-9A-Fa-f]{6})\s*$");

    let mut cards = Vec::new();
    for block in sep.split(text) {
        let block = py_strip(block);
        if block.is_empty() {
            continue;
        }
        let mut title = String::new();
        let mut stats: Vec<(String, String)> = Vec::new();
        let mut body_lines: Vec<&str> = Vec::new();
        let mut in_body = false;
        for line in splitlines(block) {
            if title.is_empty() && line.starts_with('#') {
                title = py_strip(line.trim_start_matches('#')).to_string();
                continue;
            }
            if !in_body && let Some(m) = stat_re.captures(py_strip(line)) {
                stats.push((m[1].to_string(), m[2].to_string()));
                continue;
            }
            if py_strip(line).is_empty() && !stats.is_empty() && !in_body {
                in_body = true;
                continue;
            }
            body_lines.push(line);
        }
        let mut paragraphs: Vec<String> = Vec::new();
        let mut cur: Vec<&str> = Vec::new();
        for line in body_lines {
            if !py_strip(line).is_empty() {
                cur.push(py_strip(line));
            } else if !cur.is_empty() {
                paragraphs.push(cur.join(" "));
                cur.clear();
            }
        }
        if !cur.is_empty() {
            paragraphs.push(cur.join(" "));
        }
        let body = paragraphs.join("\n");

        let mut kind = stats
            .iter()
            .find(|(k, _)| k.to_lowercase() == "type")
            .map(|(_, v)| v.clone())
            .unwrap_or_default();
        let color = if let Some(m) = hex_re.captures(&kind) {
            let h = &m[1];
            let byte =
                |i: usize| f64::from(u8::from_str_radix(&h[i..i + 2], 16).unwrap_or(0)) / 255.0;
            let c = [byte(0), byte(2), byte(4)];
            let start = m.get(0).expect("whole match").start();
            kind = py_strip(&kind[..start]).to_string();
            c
        } else {
            let first = kind.to_lowercase().split_whitespace().next().unwrap_or("").to_string();
            type_color(&first).unwrap_or(DEFAULT_COLOR)
        };
        let copies = stats
            .iter()
            .find(|(k, _)| k.to_lowercase() == "copies")
            .map_or(Some(1), |(_, v)| py_int(v))
            .map_or(1, |n| n.max(1)) as u32;
        cards.push(Card {
            title,
            kind,
            stats: stats
                .into_iter()
                .filter(|(k, _)| !matches!(k.to_lowercase().as_str(), "type" | "copies"))
                .collect(),
            body,
            color,
            copies,
        });
    }
    cards
}

// ── inline markup & line flow ──────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Token {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

impl Token {
    fn style(&self) -> FontStyle {
        FontStyle::from_flags(self.bold, self.italic)
    }
    fn is_space(&self) -> bool {
        self.text == " "
    }
}

/// Inline markdown → word tokens (Python `_inline_tokens`). Spaces become " ".
/// Stray `*` / `_` that don't form a pair are dropped, as in Python.
pub fn inline_tokens(text: &str) -> Vec<Token> {
    static INLINE: OnceLock<Regex> = OnceLock::new();
    static WS: OnceLock<Regex> = OnceLock::new();
    let inline = re(&INLINE, r"(?s)\*\*(.+?)\*\*|__(.+?)__|\*(.+?)\*|_(.+?)_|([^*_]+)");
    let ws = re(&WS, r"\s+");
    let mut out = Vec::new();
    for m in inline.captures_iter(text) {
        let (seg, b, i, u) = if let Some(g) = m.get(1) {
            (g.as_str(), true, false, false)
        } else if let Some(g) = m.get(2) {
            (g.as_str(), false, false, true)
        } else if let Some(g) = m.get(3) {
            (g.as_str(), false, true, false)
        } else if let Some(g) = m.get(4) {
            (g.as_str(), false, true, false)
        } else {
            (m.get(5).map_or("", |g| g.as_str()), false, false, false)
        };
        // re.split(r"(\s+)") keeping separators.
        let mut last = 0;
        for sp in ws.find_iter(seg) {
            if sp.start() > last {
                out.push(Token {
                    text: seg[last..sp.start()].into(),
                    bold: b,
                    italic: i,
                    underline: u,
                });
            }
            out.push(Token { text: " ".into(), bold: false, italic: false, underline: false });
            last = sp.end();
        }
        if last < seg.len() {
            out.push(Token { text: seg[last..].into(), bold: b, italic: i, underline: u });
        }
    }
    out
}

/// Greedy word wrap (Python `_token_lines`).
pub fn token_lines(tokens: &[Token], font_size: f64, max_w: f64) -> Vec<Vec<Token>> {
    let space_w = width(" ", FontStyle::Regular, font_size);
    let mut lines = Vec::new();
    let mut line: Vec<Token> = Vec::new();
    let mut line_w = 0.0;
    for t in tokens.iter().filter(|t| !t.is_space()) {
        let w = width(&t.text, t.style(), font_size);
        let gap = if line.is_empty() { 0.0 } else { space_w };
        if line_w + gap + w <= max_w {
            line.push(t.clone());
            line_w += gap + w;
        } else {
            if !line.is_empty() {
                lines.push(std::mem::take(&mut line));
            }
            line = vec![t.clone()];
            line_w = w;
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BodyItem {
    Line(Vec<Token>),
    Gap,
}

/// Body text → wrapped lines and paragraph gaps (Python `_flow_body_items`).
pub fn flow_body(body: &str, font_size: f64, max_w: f64) -> Vec<BodyItem> {
    let paras: Vec<&str> = body.split('\n').collect();
    let mut items = Vec::new();
    for (pi, para) in paras.iter().enumerate() {
        if !para.is_empty() {
            for ln in token_lines(&inline_tokens(para), font_size, max_w) {
                items.push(BodyItem::Line(ln));
            }
        }
        if pi < paras.len() - 1 && !para.is_empty() {
            items.push(BodyItem::Gap);
        }
    }
    items
}

/// Total body height (Python `_body_height`).
pub fn body_height(body: &str, font_size: f64, max_w: f64) -> f64 {
    let lh = font_size * 1.4;
    let gap = lh * 0.5;
    let paras: Vec<&str> = body.split('\n').collect();
    let mut h = 0.0;
    for (pi, p) in paras.iter().enumerate() {
        if !p.is_empty() {
            h += token_lines(&inline_tokens(p), font_size, max_w).len() as f64 * lh;
        }
        if pi < paras.len() - 1 && !p.is_empty() {
            h += gap;
        }
    }
    h
}

/// Python `_sim_body_start_y`: where the body would start, in card coordinates.
/// (Python's simulation differs slightly from drawing for empty stat values and
/// the second divider; it is reproduced exactly because it decides overflow.)
pub fn sim_body_start_y(card: &Card) -> f64 {
    let mut stat_y = CARD_H - 18.0 - 16.0 - 4.0 - 10.0;
    let bottom_guard = 30.0;
    for (key, val) in &card.stats {
        if stat_y < bottom_guard {
            return stat_y;
        }
        let key_w = width(&format!("{key}: "), FontStyle::Bold, STAT_FONT);
        let mut cur = String::new();
        let mut first = true;
        for word in val.split_whitespace() {
            let test = py_strip(&format!("{cur} {word}")).to_string();
            let avail = if first { LINE_W - key_w } else { LINE_W - 4.0 };
            if width(&test, FontStyle::Regular, STAT_FONT) <= avail {
                cur = test;
            } else {
                stat_y -= STAT_LINE_H;
                first = false;
                cur = word.to_string();
            }
        }
        stat_y -= STAT_LINE_H;
    }
    if !card.stats.is_empty() && !card.body.is_empty() {
        stat_y -= 11.0;
    }
    stat_y
}

/// Indices of cards whose body overflows even at 9 pt (Python `find_overflow_cards`).
pub fn find_overflow(cards: &[Card]) -> Vec<usize> {
    let min_fs = BODY_FONT_SIZES[BODY_FONT_SIZES.len() - 1];
    cards
        .iter()
        .enumerate()
        .filter(|(_, c)| !c.body.is_empty())
        .filter(|(_, c)| {
            let available = sim_body_start_y(c) - PAD;
            available <= 0.0 || body_height(&c.body, min_fs, LINE_W) > available
        })
        .map(|(i, _)| i)
        .collect()
}

// ── drawing ops ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Op {
    /// Stroked rectangle.
    Rect {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        width: f64,
        rgb: Rgb,
    },
    /// Filled rectangle.
    Fill {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        rgb: Rgb,
    },
    Line {
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        width: f64,
        rgb: Rgb,
        dash: Option<[f64; 2]>,
    },
    /// Text with its baseline at `y` starting at `x`.
    Text {
        x: f64,
        y: f64,
        text: String,
        style: FontStyle,
        size: f64,
        rgb: Rgb,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageOps {
    /// `true` for the back side of the previous sheet (duplex continuation).
    pub back: bool,
    pub ops: Vec<Op>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeckLayout {
    pub page_w: f64,
    pub page_h: f64,
    pub margin_x: f64,
    pub margin_y: f64,
    pub pages: Vec<PageOps>,
    /// Card count after expanding copies.
    pub cards: usize,
    /// Titles of cards whose text does not fit the front (even at 9 pt).
    pub overflow: Vec<String>,
    /// Whether back pages were generated.
    pub backs: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Flip {
    /// Long-edge duplex: columns mirrored.
    #[default]
    Long,
    /// Short-edge duplex: rows mirrored.
    Short,
}

const BLACK: Rgb = [0.0, 0.0, 0.0];
const WHITE: Rgb = [1.0, 1.0, 1.0];
const BODY: Rgb = [0.1, 0.1, 0.1];

fn truncate_title(title: &str) -> String {
    let orig = if title.is_empty() { "Untitled" } else { title };
    let mut t: Vec<char> = orig.chars().collect();
    while width(&t.iter().collect::<String>(), FontStyle::Bold, 11.0) > LINE_W && t.len() > 4 {
        t.pop();
    }
    let s: String = t.iter().collect();
    if s != orig {
        t.pop();
        return t.iter().collect::<String>() + "…";
    }
    s
}

fn banner_and_title(ops: &mut Vec<Op>, x: f64, y: f64, card: &Card) -> f64 {
    ops.push(Op::Rect { x, y, w: CARD_W, h: CARD_H, width: 0.75, rgb: [0.3; 3] });
    let by = y + CARD_H - BANNER_H;
    ops.push(Op::Fill { x, y: by, w: CARD_W, h: BANNER_H, rgb: card.color });
    let label = if card.kind.is_empty() { "CARD".to_string() } else { card.kind.to_uppercase() };
    ops.push(Op::Text {
        x: x + PAD,
        y: by + 5.0,
        text: label,
        style: FontStyle::Bold,
        size: 8.0,
        rgb: WHITE,
    });
    by
}

fn title_and_divider(ops: &mut Vec<Op>, x: f64, by: f64, card: &Card) -> f64 {
    let title_y = by - 16.0;
    ops.push(Op::Text {
        x: x + PAD,
        y: title_y,
        text: truncate_title(&card.title),
        style: FontStyle::Bold,
        size: 11.0,
        rgb: BLACK,
    });
    let divider_y = title_y - 4.0;
    ops.push(Op::Line {
        x1: x + PAD,
        y1: divider_y,
        x2: x + CARD_W - PAD,
        y2: divider_y,
        width: 0.5,
        rgb: [0.7; 3],
        dash: None,
    });
    divider_y
}

fn draw_token_line(ops: &mut Vec<Op>, line: &[Token], x: f64, y: f64, fs: f64) {
    let space_w = width(" ", FontStyle::Regular, fs);
    let mut cx = x;
    for (j, t) in line.iter().enumerate() {
        let w = width(&t.text, t.style(), fs);
        ops.push(Op::Text {
            x: cx,
            y,
            text: t.text.clone(),
            style: t.style(),
            size: fs,
            rgb: BODY,
        });
        if t.underline {
            ops.push(Op::Line {
                x1: cx,
                y1: y - 1.0,
                x2: cx + w,
                y2: y - 1.0,
                width: 0.4,
                rgb: BODY,
                dash: None,
            });
        }
        cx += w + if j < line.len() - 1 { space_w } else { 0.0 };
    }
}

/// Draws body items top-down; returns the index of the first item that didn't fit.
fn draw_body_items(
    ops: &mut Vec<Op>,
    items: &[BodyItem],
    x: f64,
    top_y: f64,
    bottom_y: f64,
    fs: f64,
) -> usize {
    let lh = fs * 1.4;
    let gap = lh * 0.5;
    let mut y = top_y;
    for (i, it) in items.iter().enumerate() {
        match it {
            BodyItem::Line(ln) => {
                if y < bottom_y {
                    return i;
                }
                draw_token_line(ops, ln, x, y, fs);
                y -= lh;
            }
            BodyItem::Gap => y -= gap,
        }
    }
    items.len()
}

/// Draws a card front (Python `_draw_card`). Returns the font size and leftover
/// body items when the body overflowed.
fn draw_card(ops: &mut Vec<Op>, x: f64, y: f64, card: &Card) -> Option<(f64, Vec<BodyItem>)> {
    let by = banner_and_title(ops, x, y, card);
    let divider_y = title_and_divider(ops, x, by, card);

    let mut stat_y = divider_y - 10.0;
    let bottom_guard = y + 30.0;
    'stats: for (key, val) in &card.stats {
        if stat_y < bottom_guard {
            break;
        }
        let key_str = format!("{key}: ");
        let key_w = width(&key_str, FontStyle::Bold, STAT_FONT);
        let cont_indent = PAD + 4.0;
        ops.push(Op::Text {
            x: x + PAD,
            y: stat_y,
            text: key_str,
            style: FontStyle::Bold,
            size: STAT_FONT,
            rgb: BLACK,
        });
        let mut cur = String::new();
        let mut first = true;
        for word in val.split_whitespace() {
            let test = py_strip(&format!("{cur} {word}")).to_string();
            let avail = if first { LINE_W - key_w } else { LINE_W - (cont_indent - PAD) };
            if width(&test, FontStyle::Regular, STAT_FONT) <= avail {
                cur = test;
            } else {
                let dx = if first { x + PAD + key_w } else { x + cont_indent };
                if !cur.is_empty() {
                    ops.push(Op::Text {
                        x: dx,
                        y: stat_y,
                        text: cur.clone(),
                        style: FontStyle::Regular,
                        size: STAT_FONT,
                        rgb: BLACK,
                    });
                }
                stat_y -= STAT_LINE_H;
                if stat_y < bottom_guard {
                    continue 'stats;
                }
                first = false;
                cur = word.to_string();
            }
        }
        if !cur.is_empty() {
            let dx = if first { x + PAD + key_w } else { x + cont_indent };
            ops.push(Op::Text {
                x: dx,
                y: stat_y,
                text: cur,
                style: FontStyle::Regular,
                size: STAT_FONT,
                rgb: BLACK,
            });
            stat_y -= STAT_LINE_H;
        }
    }

    if !card.stats.is_empty() && !card.body.is_empty() {
        let div2 = stat_y + 3.0;
        ops.push(Op::Line {
            x1: x + PAD,
            y1: div2,
            x2: x + CARD_W - PAD,
            y2: div2,
            width: 0.5,
            rgb: [0.7; 3],
            dash: None,
        });
        stat_y = div2 - 11.0;
    }

    if !card.body.is_empty() && stat_y > y + PAD {
        let available = stat_y - (y + PAD);
        let mut chosen = BODY_FONT_SIZES[BODY_FONT_SIZES.len() - 1];
        for fs in BODY_FONT_SIZES {
            chosen = fs;
            if body_height(&card.body, fs, LINE_W) <= available {
                break;
            }
        }
        let items = flow_body(&card.body, chosen, LINE_W);
        let drawn = draw_body_items(ops, &items, x + PAD, stat_y, y + PAD, chosen);
        if drawn < items.len() {
            let leftover: Vec<BodyItem> =
                items[drawn..].iter().skip_while(|i| matches!(i, BodyItem::Gap)).cloned().collect();
            if !leftover.is_empty() {
                return Some((chosen, leftover));
            }
        }
    }
    None
}

/// Draws a back card carrying the leftover body (Python `_draw_card_back`).
fn draw_card_back(ops: &mut Vec<Op>, x: f64, y: f64, card: &Card, fs: f64, items: &[BodyItem]) {
    let by = banner_and_title(ops, x, y, card);
    let cont_w = width("CONT.", FontStyle::Italic, 7.0);
    ops.push(Op::Text {
        x: x + CARD_W - PAD - cont_w,
        y: by + 5.0,
        text: "CONT.".into(),
        style: FontStyle::Italic,
        size: 7.0,
        rgb: WHITE,
    });
    let divider_y = title_and_divider(ops, x, by, card);
    draw_body_items(ops, items, x + PAD, divider_y - 11.0, y + PAD, fs);
}

fn cutting_guides(ops: &mut Vec<Op>, pw: f64, ph: f64, mx: f64, my: f64) {
    let (rgb, dash) = ([0.6; 3], Some([2.0, 3.0]));
    for col in 0..=COLS {
        let gx = mx + col as f64 * CARD_W;
        ops.push(Op::Line { x1: gx, y1: 0.0, x2: gx, y2: ph, width: 0.3, rgb, dash });
    }
    for row in 0..=ROWS {
        let gy = my + row as f64 * CARD_H;
        ops.push(Op::Line { x1: 0.0, y1: gy, x2: pw, y2: gy, width: 0.3, rgb, dash });
    }
}

/// Lays out the whole deck (Python `export_pdf`): 3×3 per page, cutting guides
/// on every page, and — when `double_sided` and some card overflows — one back
/// page after every front page, mirrored for the chosen flip.
pub fn layout(cards: &[Card], format: PageFormat, double_sided: bool, flip: Flip) -> DeckLayout {
    let (pw, ph) = format.size_pt();
    let mx = (pw - COLS as f64 * CARD_W) / 2.0;
    let my = (ph - ROWS as f64 * CARD_H) / 2.0;
    let expanded: Vec<&Card> =
        cards.iter().flat_map(|c| std::iter::repeat_n(c, c.copies as usize)).collect();
    let owned: Vec<Card> = expanded.iter().map(|c| (*c).clone()).collect();
    let overflow_idx = find_overflow(&owned);
    let use_backs = double_sided && !overflow_idx.is_empty();

    let mut pages = Vec::new();
    for chunk in expanded.chunks(PER_PAGE) {
        let mut front = Vec::new();
        cutting_guides(&mut front, pw, ph, mx, my);
        let mut backs = Vec::new();
        for (i, card) in chunk.iter().enumerate() {
            let (col, row) = (i % COLS, i / COLS);
            let cx = mx + col as f64 * CARD_W;
            let cy = ph - my - (row + 1) as f64 * CARD_H;
            if let Some((fs, items)) = draw_card(&mut front, cx, cy, card)
                && use_backs
            {
                backs.push((col, row, *card, fs, items));
            }
        }
        pages.push(PageOps { back: false, ops: front });
        if use_backs {
            let mut back = Vec::new();
            cutting_guides(&mut back, pw, ph, mx, my);
            for (col, row, card, fs, items) in backs {
                let (bcol, brow) = match flip {
                    Flip::Short => (col, ROWS - 1 - row),
                    Flip::Long => (COLS - 1 - col, row),
                };
                let bx = mx + bcol as f64 * CARD_W;
                let by = ph - my - (brow + 1) as f64 * CARD_H;
                draw_card_back(&mut back, bx, by, card, fs, &items);
            }
            pages.push(PageOps { back: true, ops: back });
        }
    }
    DeckLayout {
        page_w: pw,
        page_h: ph,
        margin_x: mx,
        margin_y: my,
        cards: expanded.len(),
        overflow: overflow_idx.iter().map(|&i| owned[i].title.clone()).collect(),
        backs: use_backs,
        pages,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIREBALL: &str = "# Fireball\n**Type:** Spell\n**Level:** 3rd-level\n**Copies:** 2\n\nA bright **streak**.\n\nSecond para.";

    #[test]
    fn parses_title_stats_body_type_and_copies() {
        let c = &parse(FIREBALL)[0];
        assert_eq!(c.title, "Fireball");
        assert_eq!(c.kind, "Spell");
        assert_eq!(c.stats, vec![("Level".to_string(), "3rd-level".to_string())]);
        assert_eq!(c.body, "A bright **streak**.\nSecond para.");
        assert_eq!(c.color, [0.5, 0.2, 0.7]);
        assert_eq!(c.copies, 2);
    }

    #[test]
    fn custom_hex_colour_and_unknown_type() {
        let c = &parse("# Boss\n**Type:** Boss #8B0000\n\nx")[0];
        assert_eq!(c.kind, "Boss");
        assert!((c.color[0] - 139.0 / 255.0).abs() < 1e-12);
        assert_eq!(parse("# X\n**Type:** weird thing\n\nb")[0].color, DEFAULT_COLOR);
    }

    #[test]
    fn copies_parsing_is_forgiving() {
        assert_eq!(parse("# A\n**Copies:** many\n\nb")[0].copies, 1);
        assert_eq!(parse("# A\n**Copies:** 0\n\nb")[0].copies, 1);
        assert_eq!(parse("# A\n**Copies:**  +3 \n\nb")[0].copies, 3);
    }

    #[test]
    fn inline_tokens_follow_python_regex_order() {
        let t = inline_tokens("**bold** and *it* __u__ _i_");
        let words: Vec<(&str, bool, bool, bool)> = t
            .iter()
            .filter(|t| t.text != " ")
            .map(|t| (t.text.as_str(), t.bold, t.italic, t.underline))
            .collect();
        assert_eq!(
            words,
            [
                ("bold", true, false, false),
                ("and", false, false, false),
                ("it", false, true, false),
                ("u", false, false, true),
                ("i", false, true, false)
            ]
        );
        // Unpaired markers are dropped, like Python's finditer skipping them.
        let stray: Vec<String> = inline_tokens("a**b").into_iter().map(|t| t.text).collect();
        assert_eq!(stray, ["a", "b"]);
    }

    #[test]
    fn long_titles_are_truncated_with_ellipsis() {
        let t = truncate_title("An extraordinarily long card title that cannot fit");
        assert!(t.ends_with('…'));
        assert!(width(&t, FontStyle::Bold, 11.0) <= LINE_W + width("…", FontStyle::Bold, 11.0));
        assert_eq!(truncate_title(""), "Untitled");
    }

    #[test]
    fn layout_pages_and_duplex_backs() {
        let long = format!("# Long\n**Type:** Spell\n\n{}", "word ".repeat(400));
        let cards = parse(&format!("{long}\n---\n# Short\n**Type:** Feat\n\nTiny."));
        let single = layout(&cards, PageFormat::A4, false, Flip::Long);
        assert_eq!((single.pages.len(), single.backs), (1, false));
        assert_eq!(single.overflow, vec!["Long".to_string()]);
        let duplex = layout(&cards, PageFormat::A4, true, Flip::Long);
        assert_eq!(duplex.pages.len(), 2);
        assert!(duplex.pages[1].back);
        // Long-edge flip mirrors columns: card in column 0 continues in column 2.
        let back_banner_x = duplex.pages[1]
            .ops
            .iter()
            .find_map(|o| match o {
                Op::Fill { x, .. } => Some(*x),
                _ => None,
            })
            .unwrap();
        assert!((back_banner_x - (duplex.margin_x + 2.0 * CARD_W)).abs() < 1e-9);
        let short = layout(&cards, PageFormat::A4, true, Flip::Short);
        let y = short.pages[1]
            .ops
            .iter()
            .find_map(|o| if let Op::Fill { y, .. } = o { Some(*y) } else { None })
            .unwrap();
        assert!(
            (y - (short.margin_y + CARD_H - BANNER_H)).abs() < 1e-9,
            "short-edge: row 0 → row 2 (bottom)"
        );
    }

    #[test]
    fn duplex_without_overflow_adds_no_backs() {
        let cards = parse("# A\n\nshort body");
        let l = layout(&cards, PageFormat::Letter, true, Flip::Long);
        assert_eq!((l.pages.len(), l.backs), (1, false));
        assert!((l.margin_x - 36.0).abs() < 1e-9 && (l.margin_y - 18.0).abs() < 1e-9);
    }

    #[test]
    fn copies_expand_across_pages() {
        let cards = parse("# A\n**Copies:** 10\n\nx");
        let l = layout(&cards, PageFormat::A4, false, Flip::Long);
        assert_eq!((l.cards, l.pages.len()), (10, 2));
    }
}
