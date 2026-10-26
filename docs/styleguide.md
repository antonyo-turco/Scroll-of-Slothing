# Styleguide: collections

How the app shows a set of things: lists, list detail, pickers, pills, chips, tile grids and the
switch between views. All of it lives in `app/src/app.css` (section "Collections"). A view sets
only its own `grid-template-columns` and the odd layout detail; it does not restyle rows, pills or
chips. The page layout around them (top bar, toolbar band, inspector) is in `ux.md`, "Layout".

## Type and space
Roomy, large type (the user's preference). Font sizes come from the scale in `app.css`, never raw
pixels:

| Token | Size | For |
|---|---|---|
| `--fs-xs` | 13 px | labels, meta, notes, status line, pills, chips, column headers |
| `--fs-sm` | 14 px | secondary body text |
| `--fs-md` | 15 px | body text (the root size) and list rows |
| `--fs-lg` | 17 px | section and card titles |
| `--fs-xl` | 20 px | view and dialog titles |

Nothing is smaller than 13 px, except count badges on the rail and canvas and the `kbd` hint
(12 px). Bigger display numbers (clock counts, initiative, the XP total) can use their own size.
Rows are at least 56 px tall; dialogs can be wide (1000–1400 px) when the content gains from it.

Shared helpers instead of per-view copies: `.hint` (a short muted line that has to stay
visible), `.small`, `.mono`, `.muted`. Views don't redefine `.field`, `.hint` or font sizes.

## Panes
A view with tabs of its own (Music) puts each tab in a `.pane`: its toolbar band, then a scrolling
`.pane-body`. An empty `.pane-body` centres its `.empty-state`, like every other view.

## Explanations go behind an ⓘ
A section or toolbar never carries a line of explanation under it. Put it in an
`<InfoTip label="About …" text="…" />` next to the title or at the end of the toolbar. Empty
states still say what to do next (`.empty-state`), since that is an action, not an explanation.

## The switch between views
When the same items can be shown more than one way (cards, list, pills; board, detail), the view
switch is an icon `.segmented.views` at the right end of the toolbar band, with `aria-pressed`
buttons, an `aria-label` per view, and the choice saved in a workspace key (`<route>.view`,
through `api.workspaceSet`) so it is remembered.

## Lists
A table of items with a header row. Use it for anything you scan, sort or compare.

```html
<div class="list-wrap">                                <!-- scrolls; the header sticks -->
  <div class="list" role="table" aria-label="…">
    <div class="list-head" role="row" style:grid-template-columns={COLUMNS}>
      <span role="columnheader" aria-sort={sort.aria('name')}>
        <button class="sort" onclick={() => sort.by('name')}>Name{sort.arrow('name')}</button>
      </span>
      …
    </div>
    <div class="list-row" class:selected role="row" tabindex="0" style:grid-template-columns={COLUMNS}>
      <span role="cell" class="list-name">
        <span class="list-title">Mage tower</span>
        <span class="list-sub">Mage, 3 × Goblin</span>
      </span>
      <span role="cell"><span class="pill">Encounter</span></span>
      <span role="cell" class="list-actions">…</span>
      <span role="cell"><button class="list-star">☆</button></span>
    </div>
  </div>
</div>
```

- **Every column header sorts.** Clicking a header sorts by that column; clicking it again flips
  the direction. The active column shows ▲ (ascending) or ▼ (descending) and `aria-sort`. Use
  `ColumnSort` from `app/src/lib/column-sort.svelte.ts`: name-like columns start A–Z, numbers and
  dates start biggest/newest first. Columns with nothing to sort by (tags, actions) are plain
  text. Pinned items stay on top whatever the sort.
- `.list-name` is the first cell: `.list-title` over a muted `.list-sub`. `.list-thumb` (44 px)
  goes before it when items have an image.
- `.list-actions` holds the row's icon buttons in same-size slots, so every row lines up. A row
  that lacks an action keeps its place with `<span class="slot"></span>`. Keep it to the main
  actions (start, add, edit, delete); the rest go in the row detail.
- `.list-star` (☆/★, `.on`) is the last column when items can be pinned or made favourite.
- Clicking a row selects it (`.selected`); double-click or Enter runs its main action.

## List with detail
Two ways to show more about the selected item:
- **Inspector**: the item's details and settings in `aside.inspector` on the right. For views where
  you edit the item (Library, Clocks).
- **Row detail**: clicking a row opens a `.list-more` bar under it with big labelled buttons (icon
  above a short label) for the secondary actions; clicking the row again closes it. For views
  where the row's actions don't fit as icons (Encounters). Toggle buttons use `aria-pressed`.

## List boxes (pickers)
In a dialog, a bordered scrolling list: `ul.list-box` (or a `div` with `role="listbox"`) holding
`.list-row`s. A row that is one button is `button.list-row`; picked rows get `.selected`.

## Pills
Small read-only labels: kind, source, state. `.pill` (grey); `.pill.accent` for tags and
highlights, `.pill.ok`, `.pill.warn`, `.pill.bad` for states. `.list-pill` is the same as `.pill`.
Pills are never buttons.

## Chips
Toggle buttons for filters and tags: `button.chip` with `aria-pressed`, in a `.chip-bar`. A
pressed chip is accent-tinted and bold. The tag input (`TagInput.svelte`) uses its own removable
chips inside the field.

## Tiles
A responsive grid of cards: `.tile-grid` (set `--tile-min`, default 240 px) of `.tile`s. `.active`
marks the current one (accent border), `.selected` the ones picked for a bulk action (focus ring).
The Library's grid is virtualized (`VirtualGrid.svelte`) and sizes its cards itself.

## Where each pattern is used
| Pattern | Views |
|---|---|
| View switch | Library (Cards / List / Pills), Clocks (Board / Detail) |
| List (sortable) | Music → Library, Library list view (its column sorts are the Sort menu's), Combat → Encounters, Combat → Parties |
| List (rows only, in groups or editable) | Planner sections, Soundboard editor |
| Inspector detail | Library, Clocks, Token cutter, Map paginator (editors) |
| Row detail (`.list-more`) | Combat → Encounters |
| List box | Add creature, Spells and items, Creature picker (Tokens from 5etools), Planner → Add to session |
| Pills | Library cards and rows, Music library, Soundscapes (tags), Mixer (ducked, running), Encounters (kind) |
| Chips | Library tag bar, Music library filters and tags, track dialog tags |
| Tiles | Clocks board, Soundboard pads (square colour tiles, `.active` while playing); Library cards (virtualized) |

## Kept apart on purpose
Combat's tracker table, the stat block and the condition chips (`--c` colour per condition) are
domain views with their own look. They still use the type scale.
