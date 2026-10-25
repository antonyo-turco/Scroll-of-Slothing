# Settings

Global preferences, stored in the `settings` table (key → JSON) with typed defaults in Rust. Changes are applied **live to every view**. Python only pushed them to the token sheet and map paginator.

## Layout
The page uses vertical tabs, one per feature: General, Cut guides, Tokens, Cut-outs & cards,
Maps, Transcription, Combat and Data. Advanced options sit in accordions: the in-app MCP server
at the bottom of General, the cloud model id under Transcription, the encounter import options
under Combat.

## Keys and defaults
| key | type | default | validation | used by |
|---|---|---|---|---|
| `start_view` | route id | `tokens.sheet` | must be a known route | app start |
| `sidebar_collapsed` | bool | false | | shell |
| `token_page_format` | `Letter`\|`A4` | `Letter` | | token sheet |
| `object_page_format` | `Letter`\|`A4` | `Letter` | | object sheet (*new key*; Python hardcoded Letter) |
| `card_page_format` | `Letter`\|`A4` | `A4` | | cards (*new key*; Python hardcoded A4) |
| `map_page_format` | `Letter`\|`A4`\|`A3` | `A4` | | map paginator, AoE (A3 is new; the other `*_page_format` keys refuse it) |
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
| `stt_engine` | `local`\|`cloud` | `local` | must be one of these | transcription (*new*) |
| `stt_cloud_base_url` | string | `https://openrouter.ai/api/v1` | http(s) URL; trimmed, trailing `/` dropped | transcription, cloud (*new*) |
| `stt_cloud_model` | string | `openai/whisper-1` | not empty; trimmed | transcription, cloud (*new*) |
| `fivetools_base_url` | string | `http://localhost:8080` | an http(s) URL; trailing `/` dropped | combat tracker: the 5etools site or mirror creatures are read from (*new*) |
| `combat_roll_init` | bool | `true` | | combat tracker: roll creature initiative (*new*) |
| `combat_roll_groups` | bool | `true` | | combat tracker: copies of a creature share one roll (*new*) |
| `combat_roll_hp` | bool | `false` | | combat tracker: roll HP instead of the average (*new*) |
| `combat_reroll_each_round` | bool | `false` | | combat tracker: reroll initiative each round (*new*) |
| `combat_hp_as_damage` | bool | `false` | | combat tracker: track HP as damage (*new*) |
| `combat_import_roll_groups` | bool | `true` | | combat tracker: encounter import: one roll per creature type (*new*) |
| `combat_import_add_players` | bool | `true` | | combat tracker: encounter import: add the default party (*new*) |
| `combat_xp_multiplier_2014` | bool | `false` | | encounter difficulty: apply the 2014 multiplier for the number of enemies (*new*) |
| `combat_turn_popup` | bool | `true` | | combat tracker: a card with whose turn it is and who is next, on each turn (*new*) |
| `combat_reset_confirm` | bool | `true` | | combat tracker: ask before Reset (*new*) |
| `combat_reset_keep_players` | bool | `false` | | combat tracker: Reset removes only creatures and keeps the players (*new*) |
| `combat_import_append` | bool | `false` | | combat tracker: encounter import: add to the current combat (*new*) |

The OpenRouter API key is **not** a setting: it is kept in the Windows Credential Manager (entry `sleuth` / `openrouter_api_key`), see `features/transcribe.md`.

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
- `start_tab`, which held a label, became `start_view`, which holds a route id. The legacy importer maps the old labels to routes. The five Python music tabs (Library, Mixer, Soundscapes, Playlists, Soundboard) map to the single `music` route. Unknown labels fall back to the default.
- Legacy values that Python accepted but that break later calculations are reset to their defaults on import. For example, `map_square_in: false` was coerced to 0.0 and then divided by. Coercion itself matches Python exactly; see the golden test `crates/core/tests/parity_settings.rs`.

## Parity checklist (Python `app_settings.py`, `ui/settings_panel.py`)
- [x] All 14 legacy keys with the same defaults (`app_settings.py:20-38`): `sleuth_core::settings` tests
- [x] Legacy coercion (`_coerce`): golden fixture `settings_coerce.json`
- [x] Cut-line dash patterns (`:14`) and the ReportLab stroke behaviour (`:98`): the sheet PDFs stroke one cut line per item with the configured dash (`sleuth_pdf::token_sheet` test `cut_border_adds_stroked_paths`, `[2 3]` for dashed)
- [x] Validation: floats ≥ 0, and square/calibration/width > 0 (`settings_panel.py:149`)
- [x] Restore defaults with confirmation
- [x] Start view and the collapsed sidebar are honoured at launch
