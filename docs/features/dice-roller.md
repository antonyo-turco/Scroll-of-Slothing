# Dice roller (floating)

A small, floating dice roller the DM can use anywhere in the app: roll from quick buttons
(e.g. `d20`) or by typing an expression (`2d6+3`) into a chat-like line, and see the latest
roll picked out even while the widget is collapsed. Requested 2026-11-04.

## Parity note

The rewrite has reached **full parity** with Orb of Many Functions: the user declared the
rewrite feature-complete and at parity on 2026-10-18, and every parity checklist is ticked
as of 2026-10-25, music included. This dice roller is **new**. Nothing like it exists in the
Python reference app, so it lives outside the old project's boundaries.

From here on, new features (this one, and any later ones) are scored against this app's own
specs, not the Python source, and do **not** need a Python parity checklist. This one has
none; its checklist is at the end.

The whole point is that it stays **out of the way**: it is small, movable, and does not cover
or steal focus from whatever the DM is doing. There is no inspector; everything lives in the
floating widget itself.

## States

- **Compact**: a pill showing only the **latest** roll (its expression and the total in an
  accent chip), plus a small hide button. With no rolls yet it reads "Roll dice". This is the
  resting state whenever the DM is doing anything else. Clicking the pill expands it.
- **Expanded**: the full widget: a header (drag grip, Clear, collapse, hide), the recent rolls,
  the quick dice buttons and the expression line.
- **Hidden**: fully dismissed, leaving the view untouched.

Transitions:
- A pointer-down or focus anywhere outside the widget collapses **Expanded → Compact**, and so
  does Escape inside it.
- Nothing auto-hides the **Compact** pill, so the latest roll stays visible. The DM hides it
  explicitly to see what is behind it.
- The rail button, the `D` key and the command palette toggle: Hidden → Expanded (with the
  expression line focused), anything else → Hidden.

## The widget

- A global overlay (`position: fixed`, `z-index` below toasts), rendered by `App.svelte` on
  every route. It is not a rail destination.
- **Rail button**: "Dice" at the bottom of the rail, above Settings (`aria-pressed` shows
  whether the widget is shown). It sits outside the Before/During/After groups because the
  widget is not a view.
- **Key**: `D` with no modifier, ignored while typing in an input, textarea, select or
  contenteditable, and on key repeat.
- **Palette**: "Dice roller: show or hide (D)".
- **Movable**: drag the pill (compact) or the header grip (expanded). A pill press that moves
  less than 4 px counts as a click and expands it. While dragging it gets a drop shadow. The
  position is clamped inside the window (12 px margin) and re-clamped on resize and when the
  widget grows, so expanding never pushes it off-screen. The default spot is the bottom-right
  corner.
- **Size**: the pill fits its content; the expanded card is 340 px wide, and the roll list
  scrolls past 240 px.
- **Persistence**: position and hidden/shown are autosaved in the `workspaces` table under
  `diceRoller` (`{ x, y, state }`, `state` = `hidden` | `compact`) and restored on relaunch. A
  widget that was open relaunches compact. History is not persisted.
- The unsent text in the expression line survives collapsing and hiding.

## Rolling

Both entry points post into the same recent-roll list:

- **Buttons**: `d4 d6 d8 d10 d12 d20 d100`, plus **Adv** and **Dis**, which roll `adv` / `dis`
  (2d20, keep the higher / lower; the detail shows both dice, the total uses the kept one).
- **The expression line**: type notation and press Enter (or Roll):
  - `NdM` and number terms joined by `+` / `-`: `2d6`, `d20`, `2d6+3`, `d20-1`, `2d8+1d6+4`;
  - `d%` is an alias for `d100`;
  - `adv` / `dis` are terms too, so `adv+5` is an attack with advantage;
  - case and whitespace are ignored, and `−` (U+2212) counts as a minus, so a shown
    expression can be pasted back.
- Limits: at most 200 dice per expression (`adv`/`dis` count as 2), at most 10 000 sides.
- Invalid expressions are rejected inline: the line shakes (no animation under reduced
  motion), an error under it explains why (`'2d0' isn't a die roll or a number`), and nothing
  is posted. The text stays so it can be fixed.
- Each roll goes through the service (`dice_roll`), so the widget and the MCP tool use one
  roller. In the browser mock the TS mirror `rollNotation` (`app/src/lib/dice.ts`)
  answers instead.

## Recent rolls

- A session-only list of the last **10** rolls, newest at the bottom: the canonical
  expression, the total (large), and the detail with each die's face (`[3, 5] + 3 = 11`,
  `[14, 7] adv + 2 = 16`). Constant-only expressions (`5`) show no detail line.
- The newest roll is picked out with the accent background and outline and a larger total.
  The list scrolls to keep it in view and is announced politely to screen readers.
- **Clear** empties the list.

## Engine (`crates/core/src/dice.rs`)

- `Expression::parse(&str) -> Result<Expression, ParseError>`: validates and lowers to signed
  terms (`NdM`, `adv`/`dis`, constants). `ParseError` is `Empty`, `Syntax(term)` or
  `TooManyDice`. `Expression::text` is the canonical form (`2d6 + 3`, `d20 − 1`, `adv + 5`),
  which parses back to the same expression.
- `Expression::bounds()`: lowest and highest possible totals.
- `roll_expr(&Expression, seed) -> Roll`: rolls with `fastrand::Rng::with_seed`, so the same
  seed gives the same roll and `core` never touches an entropy source. `Roll` holds the
  expression, the total, the detail and one `DieGroup { sides, faces, kept?, sign }` per dice
  term.
- No parentheses, multiplication, exploding dice or keep/drop operators beyond `adv`/`dis`.

## Service, Tauri, MCP

- `Service::dice_roll(notation) -> Result<RollResult>` (`crates/service/src/dice.rs`): parses
  (errors become `ServiceError::InvalidInput`), seeds from `fastrand`, rolls. It is stateless
  and writes nothing, so there is no `change_log` entry. `dice_roll_seeded` serves the tests.
- Tauri command `dice_roll { notation }`; `api.diceRoll(notation)` in the UI.
- MCP tool `dice_roll { notation }` (`docs/mcp-tools.md`): the LLM gets the same `RollResult`.
  Its rolls do not show in the widget, which only lists rolls made in it.

## Code layout

- `crates/core/src/dice.rs`: parser, roller and rendering, with unit and property tests.
- `crates/service/src/dice.rs`: `dice_roll`, with seeded tests.
- `app/src-tauri/src/commands.rs`: `dice_roll`.
- `crates/mcp/src/lib.rs`: the `dice_roll` tool.
- `app/src/lib/dice.ts`: `parseNotation` / `rollNotation`, the TS mirror of the core (mock
  backend), next to the combat tracker's `rollFormula` / `rollAttack`; `dice.test.ts` checks
  that it matches the core's canonical text and errors.
- `app/src/lib/dice.svelte.ts`: widget state (mode, position, recent rolls, error, persistence).
- `app/src/lib/components/DiceRoller.svelte`: the overlay, mounted in `App.svelte`; the `D` key
  is in `App.svelte`, the button in `Rail.svelte`, the command in `CommandPalette.svelte`.
- `app/e2e/dice.spec.ts`: Playwright smoke test.

## Scope / not in v1

- Roll history across sessions, or a log of rolls made through MCP.
- Game-system operators (exploding dice, `kh`/`kl`, rerolls, drop-lowest for stats) beyond the
  `adv`/`dis` terms; crit doubling stays in the combat tracker's `rollFormula`.
- Per-die colouring, 3D dice, sounds, notifications, or sharing rolls.
- Binding rolls to creatures in the combat tracker, or a "roll initiative" helper.
- A tens/ones split for `d%`: it shows the d100 face.

## Checklist

- [x] Core parser, roller and rendering with unit and property tests
- [x] Service `dice_roll` with seeded tests
- [x] Tauri command and MCP tool, documented in `docs/mcp-tools.md`
- [x] TS mirror for the mock, with tests against the core's canonical text and errors
- [x] Widget: Compact / Expanded / Hidden, drag, quick dice, Adv/Dis, expression line, errors,
      recent rolls, Clear
- [x] Rail button, `D` key and palette command
- [x] Position and hidden/shown persisted in `workspaces`
- [x] Playwright smoke test (`app/e2e/dice.spec.ts`)
- [ ] Tried in the desktop app by the user
