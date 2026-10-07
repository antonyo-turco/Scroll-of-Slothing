# Settings

Global preferences, stored in the `settings` table (key → JSON) with typed defaults in Rust. Changes are applied **live to every view**. Python only pushed them to the token sheet and map paginator.

## Keys and defaults
| key | type | default | validation | used by |
|---|---|---|---|---|
| `start_view` | route id | `tokens.sheet` | must be a known route | app start |
| `sidebar_collapsed` | bool | false | | shell |
| `token_page_format` | `Letter`\|`A4` | `Letter` | | token sheet |
| `object_page_format` | `Letter`\|`A4` | `Letter` | | object sheet (*new key*; Python hardcoded Letter) |
| `card_page_format` | `Letter`\|`A4` | `A4` | | cards (*new key*; Python hardcoded A4) |
| `map_page_format` | `Letter`\|`A4` | `A4` | | map paginator, AoE |
| `token_cut_border` | bool | false | | token sheet |
| `map_cut_border` | bool | false | | map paginator |
| `cut_line_style` | `Dashed`\|`Dotted`\|`Solid` | `Dashed` | must be one of these | all cut guides |
| `cut_line_color` | `#rrggbb` | `#808080` | hex; falls back to grey 0.5 | |
| `cut_line_width` | f32 pt | 0.5 | > 0; drawn at `max(0.1, w)` | |
| `map_square_in` | f32 | 1.0 | > 0 | map |
| `map_calibration_mm` | f32 | 25.4 | > 0; scale factor = 25.4 / mm | map |
| `map_margin_in` | f32 | 0.0 | ≥ 0 | map |
| `map_overlap_in` | f32 | 0.0 | ≥ 0 | map |
| `map_align_to_grid` | bool | false | | map |
| `theme` | `system`\|`light`\|`dark` | `system` | | shell (*new*) |
| `mcp_http_enabled` | bool | false | | MCP (*new*) |

**Dash patterns**
| Style | PDF dash (pt) | Canvas preview dash |
|---|---|---|
| Dashed | `[2,3]` | `[3,2]` |
| Dotted | `[0.5,2]` | `[1,2]` |
| Solid | none | none |

**Coercion rules** (match Python `app_settings._coerce`):
- A value of the wrong type falls back to the default.
- A bool must be a real bool.
- Unknown keys are ignored.

## UI
Settings sit at the bottom of the rail and open as one page with these sections:
- General
- Cut guide line (style, width, colour picker, with a live preview line)
- Tokens
- Cut-outs
- Cards
- Maps
- Advanced (MCP)

Other controls:
- **Restore defaults** asks for confirmation.
- Saving is automatic, with an inline "Saved" toast. There is no Save button.
- Invalid numbers show an inline error and are not saved.

## MCP
- `settings_get` returns everything.
- `settings_set({key: value, ...})` validates the values and returns the errors per key.

## Intentional changes
- Saving is automatic, and settings apply live everywhere.
- New keys: `object_page_format`, `card_page_format`, `theme`, `mcp_http_enabled`.
- `start_tab`, which held a label, became `start_view`, which holds a route id. The legacy importer maps the old labels to routes. Music tabs, which no longer exist, fall back to the default.
- Legacy values that Python accepted but that break later calculations are reset to their defaults on import. For example, `map_square_in: false` was coerced to 0.0 and then divided by. Coercion itself matches Python exactly; see the golden test `crates/core/tests/parity_settings.rs`.

## Parity checklist (Python `app_settings.py`, `ui/settings_panel.py`)
- [x] All 14 legacy keys with the same defaults (`app_settings.py:20-38`): `sleuth_core::settings` tests
- [x] Legacy coercion (`_coerce`): golden fixture `settings_coerce.json`
- [ ] Cut-line dash patterns (`:14`) and the ReportLab stroke behaviour (`:98`). Dash constants are done and tested; stroke application lands with the PDF crate in Phase 2.
- [x] Validation: floats ≥ 0, and square/calibration/width > 0 (`settings_panel.py:149`)
- [x] Restore defaults with confirmation
- [x] Start view and the collapsed sidebar are honoured at launch
