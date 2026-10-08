# Cards

You write cards in markdown, preview them exactly as they will print, and export them to PDF as poker-size cards, 3×3 per page. Optionally the PDF is double-sided, with text that doesn't fit on the front continuing on the back.

## Deck format (markdown)
- Cards are separated by a line made only of dashes (`\n---+\n`).
- Each card:
  ```
  # Title
  **Type:** Spell            ← optional trailing #RRGGBB sets a custom colour
  **Any Stat:** value        ← any number of "**Key:** value" lines
  **Copies:** 2              ← optional, int ≥ 1, default 1
  <blank line>
  Body paragraphs, separated by blank lines.
  Inline: **bold**  __underline__  *italic*  _italic_
  ```

### Parser semantics (must match Python `parse_markdown` exactly)
1. Each block is stripped. Empty blocks are skipped.
2. **Title:** the first line starting with `#`, with the `#` characters and surrounding whitespace removed.
3. **Stats:** before the body starts, any line matching `^\*\*(.+?):\*\*\s*(.*)` (checked on the stripped line) becomes a stat.
4. **Start of body:** the first blank line *after at least one stat*. If a card has no stats, every non-title line is body text.
5. **Paragraphs:** non-empty lines are stripped and joined with a space. Blank lines end a paragraph. The paragraphs are joined with `\n`.
6. **Type and colour:** the colour comes from a trailing `#RRGGBB` in the Type value. Without one, it comes from the first word of the type, lowercased, looked up in this table:

   | Type | RGB (0–1) |
   |---|---|
   | spell | (0.5, 0.2, 0.7) |
   | feat | (0.2, 0.6, 0.3) |
   | item | (0.8, 0.5, 0.1) |
   | ability | (0.2, 0.4, 0.8) |
   | anything else | (0.4, 0.4, 0.4) |
7. **Copies:** `max(1, int)`. A value that isn't a number becomes 1.
8. The `Type` and `Copies` lines are removed from the displayed stats.

### Inline tokenizer
- The regex alternatives are tried in this order: `**b**`, `__u__`, `*i*`, `_i_`, then plain text `[^*_]+`.
- Each segment is split on whitespace into word tokens, and each word keeps its style (bold, italic, underline).
- Unmatched lone `*` or `_` characters are dropped, as in Python.

## Layout (core::cards). One engine renders both the preview and the PDF.

**Cards and page**
- Card size: 180×252 pt (2.5×3.5 in). The 3×3 grid is centred on the page.
- Margins: A4 ≈ (27.64, 42.95); Letter (36, 18).
- Card `i` on a page sits at col `i%3`, row `i/3`, with row 0 at the top.

**Front of a card**
| Element | Spec |
|---|---|
| Padding | 6 pt |
| Outer border | 0.75 pt, grey 0.3 |
| Banner | 18 pt high, filled with the card colour. Type in UPPERCASE (or "CARD" if empty), bold 8 pt, white, at (pad, banner_y+5) |
| Title | `banner_y − 16`, bold 11 pt. If it is too wide, characters are trimmed from the end (keeping more than 4) and the last character is replaced with "…". An empty title shows "Untitled". |
| Divider | 4 pt below the title. 0.5 pt, grey 0.7. Inset by the padding on both sides. |
| Stats | Start 10 pt below the divider. The key (with ": ") is bold 8.5 pt and the value regular 8.5 pt. Line height is 8.5×1.45. Values word-wrap: the first line has `line_w − key_w` available, later lines are indented to pad+4. Stats stop once y drops below `card_y + 30`. |
| Second divider | Drawn only if there are both stats and a body. Sits at `stat_y + 3`, and the body starts 11 pt below it. |
| Body | Uses the largest font size from (13, 12, 11, 10, 9) whose total height fits. Line height is fs×1.4, and paragraphs are separated by half a line. Words wrap greedily. Underline is drawn 1 pt below the baseline at 0.4 pt. A line is only drawn while `y ≥ card_y + pad`. Lines that don't fit are the *leftover*; any leading gap is dropped from it. |

**Fonts**
- Liberation Sans in Regular, Bold, Italic and BoldItalic. It is metric-compatible with Arial and Helvetica, so line breaks match Python's Helvetica.
- The same TTF is embedded in the PDF and used to measure text, so the preview and PDF are identical.

**Overflow detection**
- A card overflows if its body does not fit even at 9 pt.
- **Python quirk, reproduced on purpose:** the overflow *simulation* subtracts 11 pt for the second divider and always advances one stat line per stat, while *drawing* subtracts 8 pt (`+3 − 11`) and skips empty values. The Rust engine keeps both exactly as Python has them: the simulation decides the overflow warning and whether back pages are added, and the drawing decides what actually fits on the front. This keeps every overflow and duplex decision identical to the old app (golden `cards_layout.json`).

## Double-sided export
**When backs are added**
- Only if "double-sided" is on **and** at least one card (after expanding copies) overflows.
- Then, after each front page, one back page is emitted, even if it is empty, so the sheets stay aligned when printed duplex.

**Where the back of a card goes**
| Flip | Back position of the card at (col, row) |
|---|---|
| long-edge (default) | (2 − col, row) |
| short-edge | (col, 2 − row) |

**Back card contents**
- The same outer border and banner, plus "CONT." right-aligned in italic 7 pt.
- The title and divider.
- The leftover body text, starting 11 pt below the divider, at the **same font size** as the front.

**Every page**
- Cutting guides across the whole page: dashed `[2,3]`, 0.3 pt, grey 0.6. These are fixed and do not use the cut-line setting.

## Editor UI
**Layout:** a plain markdown text editor on the left (monospace, native undo; no editor library needed) and the exact preview on the right.
- The preview updates live, debounced 250 ms (Python only updated on Ctrl+Enter).
- It shows front and back pages and lets you page through them.
- Click a page to enlarge it.

**Template buttons:** Spell, Feat, Item, Ability.
- Each one appends its template, adding a `---` separator if the deck already has content.
- The template text matches Python's `TEMPLATES` (see below).

**Help ("?"):** the format reference, with a "Copy" button. The text is Python's `REFERENCE_TEXT`.

**Deck actions**
- Save deck and load deck as `.md` or `.txt` (UTF-8).
- Decks are also stored in the `card_decks` table, with a "Recent decks" menu.
- The current editor is autosaved in the `workspaces` table (`cards.editor`).

**Footer**
- Format: A4 or Letter. The default comes from the `card_page_format` setting, which defaults to A4.
- Double-sided toggle (default off). The Flip choice (long or short edge) only appears when it is on.
- Export PDF, which runs as a background job.
- Status line: "N cards · M pages".
- Overflow notice, in an amber warning colour:
  - when double-sided is off: "⚠ text overflow: <titles>";
  - when it is on: "↪ continues on back: <titles>".

### Templates (verbatim)
```
# Spell Name
**Type:** Spell
**Level:** 1st-level
**Casting Time:** 1 action
**Range:** 60 feet
**Components:** V, S
**Duration:** Instantaneous

Description text here.
```
```
# Feat Name
**Type:** Feat
**Prerequisite:** None

Description text here.
```
```
# Item Name
**Type:** Item
**Rarity:** Uncommon
**Attunement:** Required

Description text here.
```
```
# Ability Name
**Type:** Ability
**Recharge:** Short Rest

Description text here.
```

## MCP
| Tool | Input | Output / effect |
|---|---|---|
| `cards_templates` | | `{templates, reference}` (verbatim from Python) |
| `cards_parse` | `{markdown}` | the parsed cards |
| `cards_validate` | `{markdown, format?, double_sided?, flip?}` | `{cards, pages, overflow:[titles], backs}` |
| `cards_export_pdf` | `{markdown \| deck_id, path, format?, double_sided?, flip?}` | writes the PDF |
| `decks_list` | | the saved decks |
| `decks_get` | | one deck |
| `decks_save` | `{id?, name, markdown}` | saves a deck |
| `decks_delete` | | deletes a deck |

## Intentional changes
- The preview is exact and live, with all pages and backs. Python only showed an approximate first page, with the first 120 characters of each body.
- The page format default comes from a setting.
- Text is measured and drawn with Liberation Sans, whose glyph advances rounded to 1/1000 em are identical to Helvetica's AFM metrics (checked for all printable ASCII in all four styles), so line breaks match ReportLab exactly. Glyphs are placed without kerning, like ReportLab.
- Decks can also be saved inside the app (Saved decks), besides `.md` files.
- The editor state is autosaved and restored after a restart.
- MCP has no `cards_preview_page`: an LLM gets more from `cards_validate` (pages, overflow) and `cards_templates`.

## Parity checklist (Python `card_generator.py`, `ui/card_printer_panel.py`)
- [x] Parser: all 8 rules (golden `cards_parse.json`, exact)
- [x] Inline tokenizer order and dropping of stray markers (golden, exact)
- [x] Type colours plus custom hex colours
- [x] Copies expansion
- [x] Front layout constants (banner, title truncation, stats wrapping, body font fallback); body heights and line breaks match the golden `cards_layout.json` exactly
- [x] Overflow set: golden `cards_layout.json`, exact, with no exceptions needed
- [x] Duplex: backs only when needed, an empty back page kept, mirroring for both flips
- [x] Back card: CONT. marker, same font size
- [x] Cutting guides
- [x] Templates, reference text, save and load deck
- [x] A4 and Letter layouts
