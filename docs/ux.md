# UX guidelines

## Principles
- **One layout everywhere.** Every view has the same parts in the same places (see "Layout" below):
  - the top bar: title, sub-view tabs, and the view's file actions with the primary one last;
  - an optional toolbar band for tools and view options;
  - the canvas, preview or list in the centre, with a status line under it;
  - the inspector on the right, made of titled sections.
- **Show the common, fold the rare.** Advanced options start collapsed.
- **Never freeze.** Anything slower than about 50 ms runs as a job with progress and a cancel button. The UI only renders.
- **Nothing is lost.** Editors, sheets and the combat tracker autosave to the `workspaces` table (except transcripts, which are not saved yet).
- **Destructive actions ask for confirmation**: delete, clear all, overwrite preset, restore defaults.

## Navigation
Campaigns and sessions (the Python "Manage Campaigns" tab, the top-bar campaign picker, and every campaign or session filter) were removed at the user's request on 2026-10-11. Most material is reused across sessions; tags and the ★ Pinned star organise it instead. See `features/campaigns-sessions.md`.

The left rail has 8 destinations (plus Settings) instead of Python's 16 tabs, in three headed groups (`Destination.group` in `nav.ts`) that follow the game: Library and the session Planner on top, then **Before** the session, the print tools (Tokens to Maps); **During**, Music and Combat; **After**, Transcribe. Collapsed, the headings become separators. It can collapse to icons only, and that state is saved in the `sidebar_collapsed` setting.

| Route | Python tabs it replaces |
|---|---|
| `library` | Assets, Tagged Assets |
| `planner` | (new: the session planner) |
| `tokens.cutter`, `cutouts.cutter`, `tokens.sheet` | Token Cutter, Object Cutter, Token Sheet and Object Sheet (one sheet since 2026-10-25; `cutouts.sheet` is an alias of `tokens.sheet`) |
| `cards` | Card Printer |
| `maps.paginator`, `maps.aoe` | Map Paginator, AoE Templates |
| `music` (tabs inside the view) | Library, Mixer, Soundscapes, Playlists, Soundboard |
| `combat` | (new: the 5etools-compatible initiative tracker) |
| `clocks` | (new: Blades in the Dark style clocks; Board and Detail views with events) |
| `transcribe.file` | (new: speech to text) |
| `settings` (pinned to the bottom of the rail) | Settings |

Sub-routes appear as segmented tabs in the top bar.

## Layout
Built from shared pieces: classes in `app/src/app.css` and two components.

| Part | Where | What goes in it |
|---|---|---|
| Top bar | `TopBar.svelte` | Destination title, sub-view tabs (Tokens: Token cutter/Cut-out cutter/Sheet; Music: its five tabs), then the view's **file actions**: open, paste, save, export. The primary action is last and the only filled button. Views put them there with `<ViewActions>`; `<ViewActions slot="nav">` adds view tabs next to the title. Below 980 px the command-palette button shrinks to its icon. |
| Toolbar band | `.view-toolbar` | Tools and view options that change what the canvas shows: canvas tools, paper format, preview mode, filters, a pager. Nothing that writes files. |
| Canvas | `.stage-area` | The thing being edited or previewed. Zoomable canvases have their zoom cluster (− % + Fit) floating in the bottom-right corner (`.canvas-controls`). A tool's instructions float at its top-left. With nothing loaded it shows a drop zone (`.drop-zone`) or an empty state (`.empty-state`) that says what to do next. |
| Status line | `.statusbar` | Size, counts, page summary, gesture hints, warnings. |
| Inspector | `aside.inspector`, `var(--inspector-w)` = 320 px | Settings in `.section` blocks with a small uppercase `.section-title`. Rare options go in a collapsed `<details class="section">` ("Advanced"). When there are too many for one scroll, the inspector gets tabs (`.inspector-tabs`, the Map Paginator's Scale / Pages / Look). Destructive or end-of-list actions go in a sticky `.inspector-footer`. |

Lists, list detail, pickers, pills, chips, tile grids and view switches follow `styleguide.md`.

Rows inside sections use `.field` (label above), `.prop` (label left, 92 px), `.pair` (two columns), `.range` (label, slider, value), `.check` and `.note` (`.ok`, `.bad`, `.warn`). Suggestions use a `.callout`.

Per view:
- **Library**: filters in the toolbar band, the asset grid, and the asset details in the inspector. The inspector stays when nothing is selected and explains what clicking does. Import and Manage tags are in the top bar.
- **Token Cutter / Object Cutter**: canvas and inspector. The source image (with the colour picker) and the result preview are inspector sections. Open image, Paste, Save PNG, Add to sheet and Save to library are in the top bar.
- **Token Sheet / Object Sheet**: page preview in the centre, paper options and the pager in the toolbar band, the items on the right, Clear all in the inspector footer. Add images and Export PDF are in the top bar.
- **Map Paginator**: canvas tools in the toolbar band; the inspector has three tabs that follow the workflow: **Scale** (detect or set the square size, grid position), **Pages** (paper, print area, pages and their preview, Advanced) and **Look** (printed grid lines, embedded-grid removal, filters). Choosing a canvas tool brings up the tab it edits.
- **AoE templates**: preview mode in the toolbar band, settings in the inspector, exports in the top bar.
- **Handouts → Cards**: two equal panes (`.view.split`): the Markdown editor and the page previews, each with its toolbar band and status line. File actions are in the top bar.
- **Combat**: Tracker, Encounters and Parties tabs in the top bar; the tracker table in the centre with the Defeated list and difficulty strip under it, and the stat block side panel on the right (resizable).
- **Transcribe**: the File view: the transcript in the centre, engine and options in the inspector.
- **Music**: its five tabs are in the top bar. Each tab has a toolbar band, a scrolling body and a centred empty state. Stop Music stays at the right end of the top bar; the Mixer adds its `.dmix` actions before it.

"Save to library" in the editors always opens the same dialog (`SaveAssetDialog.svelte`). It asks for the name, the print size (tokens and cut-outs) and the tags, prefilled from the file or the library asset. For an item opened from the library it offers **Update existing** or **Save as new**. The editors have no name or size fields of their own. Tags use the shared chip input (`TagInput.svelte`), also used by the Library inspector.

## Global elements
- **Jobs tray** (`JobsTray.svelte`): each running job (export, import, sync, transcription) with its progress and a cancel button.
- **Toasts** (`Toasts.svelte`): successes and errors. Errors have a "details" expander.
- **Command palette** (`CommandPalette.svelte`, Ctrl+K): any route or action.

## Keyboard
Only these shortcuts exist; each is handled by the view named.

| Keys | Where | Action |
|---|---|---|
| Tab (first stop) | everywhere | "Skip to content", past the rail and top bar |
| Ctrl+K | everywhere | Command palette |
| Esc | dialogs, palette, ⓘ tips | Close |
| Ctrl+V | Token Cutter, Object Cutter, Map Paginator | Paste an image |
| Ctrl+S | Token Cutter, Cards | Save to library / save deck |
| Ctrl+E | Token Sheet, Object Sheet, Cards | Export PDF |
| Ctrl+F, Ctrl+A, Esc | Library | Focus search, select all, clear selection |
| Enter, Esc, Ctrl+Z | Object Cutter | Close the polygon, reset, undo the last point |
| Ctrl+Z, Ctrl+Y / Ctrl+Shift+Z | Combat | Undo, redo |
| ↑/↓ (Shift 5, Ctrl 10) | Combat HP boxes | Step the value |
| 1–9, or a button's own hotkey | Music → Soundboard | Press a button |
| Space + drag, wheel, pinch, two-finger scroll | canvases | Pan and zoom (see below) |
| Shift (held) | Map Paginator | Disable snapping (print area, reference square, page layout) |

## Canvas gestures
`app/src/lib/gestures.ts` is shared by the Map Paginator, the Token Cutter and the Object Cutter.
- WebView2 sends a touchpad pinch as wheel events with `ctrlKey` and small deltas. These zoom by `exp(−deltaY/100)`, clamped per event, so the zoom follows the fingers.
- A mouse wheel notch (`wheelDeltaY` a multiple of 120, or line-mode deltas) zooms one 10 % step.
- Any other wheel input is a touchpad scroll and pans.
- Touch pointers go through `PinchTracker`. A second finger cancels the drag or lasso point the first finger started, then pinches.

## Drag and drop
Files can be dropped anywhere on the window:
- **Editors** (token cutter, object cutter, map paginator) load the dropped file.
- **Library** opens the import dialog.
- **Sheets** add the files as items.

## Visual design
- Colours are CSS custom properties with light and dark themes, which follow the system unless set in Settings. Neutral surfaces with one accent colour.
- 8 px spacing grid. System font for the UI. Liberation Sans is used only in previews and PDFs.
- Icons: one small inline SVG set in Lucide style (`Icon.svelte`, no icon library). The Music views use emoji glyphs on some buttons (▶ ⏸ ⏹ 💾 📹), as in Python.
- Minimum target size 28 px. Focus rings are always visible. Every icon-only button has an `aria-label`.

## Accessibility
Checked by `app/e2e/a11y.spec.ts`. It runs on every `scripts/check.ps1`.
- **axe scan:** every route and every Music tab, in both light and dark themes, must have no WCAG 2.1 A/AA violations.
- **Keyboard paths:** the skip link, the rail, the command palette, and dialog focus.

What it relies on:
- The first Tab stop is "Skip to content", which focuses `<main id="content">`.
- Dialogs (`Modal.svelte`) focus their first field, keep Tab and Shift+Tab inside, close on Esc, and give focus back to whatever opened them.
- The command palette is an ARIA combobox: the input owns the keys, and `aria-activedescendant` points at the highlighted option. Esc restores the previous focus.
- Text on the accent uses `--accent-contrast` (dark, 5.5:1). The dark theme has its own `--danger`, `--success` and `--warning` so status text stays at or above 4.5:1.
- Hidden live regions that keep running (the YouTube video panel) are `inert`, not just `aria-hidden`.
- Explanations live in ⓘ tips (`InfoTip.svelte`) next to the section title or control they explain, instead of paragraphs of help text. A tip is a real button with an `aria-label` ("About …") and `aria-describedby` pointing at a `role="tooltip"`; it shows on hover and keyboard focus, closes on Esc (WCAG 1.4.13), and is `position: fixed` so scrolling panels don't clip it. Status lines, warnings, confirmations and empty states stay visible: they describe the current state, not how a control works.
- Status glyphs such as the music playback indicator carry `role="img"` and a label, and stay distinguishable with `prefers-reduced-motion`.
