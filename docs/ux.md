# UX guidelines

## Principles
- **One layout everywhere.** Every view has:
  - a top bar with the title, context actions, and one primary button;
  - a canvas or content area in the centre;
  - an inspector on the right with collapsible sections.
- **Show the common, fold the rare.** Advanced options start collapsed.
- **Never freeze.** Anything slower than about 50 ms runs as a job with progress and a cancel button. The UI only renders.
- **Nothing is lost.** Editors and sheets autosave to the `workspaces` table.
- **Destructive actions ask for confirmation**: delete, clear all, overwrite preset, restore defaults.

## Navigation
Campaigns and sessions (the Python "Manage Campaigns" tab, the top-bar campaign picker, and every campaign or session filter) were removed at the user's request on 2026-10-11. Most material is reused across sessions; tags and the ★ Pinned star organise it instead. See `features/campaigns-sessions.md`.

The left rail has 6 destinations instead of Python's 16 tabs. It can collapse to icons only, and that state is saved in the `sidebar_collapsed` setting.

| Route | Python tabs it replaces |
|---|---|
| `library` | Assets, Tagged Assets |
| `tokens.cutter`, `tokens.sheet` | Token Cutter, Token Sheet |
| `cutouts.cutter`, `cutouts.sheet` | Object Cutter, Object Sheet |
| `cards` | Card Printer |
| `maps.paginator`, `maps.aoe` | Map Paginator, AoE Templates |
| `music` (tabs inside the view) | Library, Mixer, Soundscapes, Playlists, Soundboard |
| `settings` (pinned to the bottom of the rail) | Settings |

Sub-routes appear as segmented tabs in the top bar.

## Global elements
- **Print tray**: a badge at the bottom of the rail showing how many items are on the token and object sheets. Clicking it opens that sheet.
- **Jobs tray**: shows each running export or import with progress and cancel. It shows "Open file" / "Show in folder" when a job finishes.
- **Toasts**: successes and errors. Errors include a "details" expander.

## Keyboard
| Keys | Action |
|---|---|
| Tab (first stop) | "Skip to content", which moves focus past the rail and top bar |
| Ctrl+K | Command palette (any route or action) |
| Ctrl+V | Paste an image into the current editor |
| Ctrl+Z / Ctrl+Shift+Z | Undo / redo inside editors (lasso points, map region, de-warp) |
| Ctrl+S | Save (to the library, or the current deck) |
| Ctrl+E | Export (the primary export of the view) |
| Ctrl+F | Focus search (Library) |
| Esc | Cancel the current gesture, close the drawer or dialog |
| Enter | Close the polygon (object cutter) |
| Space + drag | Pan any canvas |
| Wheel | Zoom at the cursor |
| Touchpad pinch (or Ctrl + wheel) | Zoom smoothly at the cursor |
| Touchpad two-finger scroll | Pan (Shift: sideways) |
| Touchscreen: two fingers | Pinch to zoom and pan together |
| Shift (held) | Disable snapping (map region, reference box, page layout) |
| 1–9, or a button's own hotkey | Soundboard buttons (on the Soundboard tab) |

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
- Icons: one consistent inline SVG set (Lucide); no emoji in the main UI chrome.
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
- Status glyphs such as the music playback indicator carry `role="img"` and a label, and stay distinguishable with `prefers-reduced-motion`.
