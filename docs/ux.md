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
The left rail has 7 destinations instead of Python's 16 tabs. It can collapse to icons only, and that state is saved in the `sidebar_collapsed` setting.

| Route | Python tabs it replaces |
|---|---|
| `library` | Assets, Tagged Assets |
| `tokens.cutter`, `tokens.sheet` | Token Cutter, Token Sheet |
| `cutouts.cutter`, `cutouts.sheet` | Object Cutter, Object Sheet |
| `cards` | Card Printer |
| `maps.paginator`, `maps.aoe` | Map Paginator, AoE Templates |
| `campaigns` | Manage Campaigns |
| `music` (disabled, "planned") | Library, Mixer, Soundscapes, Playlists, Soundboard |
| `settings` (pinned to the bottom of the rail) | Settings |

Sub-routes appear as segmented tabs in the top bar.

## Global elements
- **Campaign context**: a picker in the top bar, also persisted. It filters the Library and is the default campaign for new links.
- **Print tray**: a badge at the bottom of the rail showing how many items are on the token and object sheets. Clicking it opens that sheet.
- **Jobs tray**: shows each running export or import with progress and cancel. It shows "Open file" / "Show in folder" when a job finishes.
- **Toasts**: successes and errors. Errors include a "details" expander.

## Keyboard
| Keys | Action |
|---|---|
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
| Shift (held) | Disable snapping (map region, reference box, page layout) |
| 1–9 | (future) soundboard buttons |

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
