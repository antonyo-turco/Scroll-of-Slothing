// Mirrors of the Rust types serialized by the service layer.
// Keep in sync with crates/core/src/settings.rs and crates/service/src/*.rs.

/** A3 is offered for maps (Map Paginator, AoE templates) only. */
export type PageFormat = 'Letter' | 'A4' | 'A3';
export type CutLineStyle = 'Dashed' | 'Dotted' | 'Solid';
export type Theme = 'system' | 'light' | 'dark';
/** Who transcribes speech: Whisper on this computer, or an OpenRouter-format API. */
export type SttEngine = 'local' | 'cloud';

export interface Settings {
  start_view: string;
  sidebar_collapsed: boolean;
  theme: Theme;
  token_page_format: PageFormat;
  object_page_format: PageFormat;
  card_page_format: PageFormat;
  map_page_format: PageFormat;
  token_cut_border: boolean;
  map_cut_border: boolean;
  cut_line_style: CutLineStyle;
  cut_line_color: string;
  cut_line_width: number;
  map_square_in: number;
  map_calibration_mm: number;
  map_margin_in: number;
  map_overlap_in: number;
  map_align_to_grid: boolean;
  mcp_http_enabled: boolean;
  stt_engine: SttEngine;
  stt_cloud_base_url: string;
  stt_cloud_model: string;
  /** Root of the 5etools site or mirror the combat tracker reads creatures from. */
  fivetools_base_url: string;
  combat_roll_init: boolean;
  combat_roll_hp: boolean;
  combat_roll_groups: boolean;
  combat_reroll_each_round: boolean;
  combat_hp_as_damage: boolean;
  combat_import_roll_groups: boolean;
  combat_import_add_players: boolean;
  combat_import_append: boolean;
  combat_xp_multiplier_2014: boolean;
  combat_reset_confirm: boolean;
  combat_reset_keep_players: boolean;
  combat_turn_popup: boolean;
  combat_passive_initiative: boolean;
}

export interface SettingsUpdate {
  settings: Settings;
  errors: Record<string, string>;
}

export type JobState = 'running' | 'done' | 'failed' | 'cancelled';

export interface JobStatus {
  id: string;
  kind: string;
  state: JobState;
  progress: number;
  message: string;
  result: unknown;
  error: string | null;
}

export type ServiceEvent =
  | ({ type: 'job' } & JobStatus)
  | { type: 'store_changed'; entity: string; entity_id: string; op: string }
  /** In-app MCP `ui_navigate` / `ui_open_asset`. */
  | { type: 'ui_navigate'; route: string; asset_id: number | null };

export interface ServiceError {
  code: 'not_found' | 'invalid_input' | 'cancelled' | 'io' | 'internal';
  message: string;
}

export interface AppInfo {
  version: string;
  data_dir: string;
}

export const DEFAULT_SETTINGS: Settings = {
  start_view: 'tokens.sheet',
  sidebar_collapsed: false,
  theme: 'system',
  token_page_format: 'Letter',
  object_page_format: 'Letter',
  card_page_format: 'A4',
  map_page_format: 'A4',
  token_cut_border: false,
  map_cut_border: false,
  cut_line_style: 'Dashed',
  cut_line_color: '#808080',
  cut_line_width: 0.5,
  map_square_in: 1.0,
  map_calibration_mm: 25.4,
  map_margin_in: 0.0,
  map_overlap_in: 0.0,
  map_align_to_grid: false,
  mcp_http_enabled: false,
  stt_engine: 'local',
  stt_cloud_base_url: 'https://openrouter.ai/api/v1',
  stt_cloud_model: 'openai/whisper-1',
  fivetools_base_url: 'http://localhost:8080',
  combat_roll_init: true,
  combat_roll_hp: false,
  combat_roll_groups: true,
  combat_reroll_each_round: false,
  combat_hp_as_damage: false,
  combat_import_roll_groups: true,
  combat_import_add_players: true,
  combat_import_append: false,
  combat_xp_multiplier_2014: false,
  combat_reset_confirm: true,
  combat_reset_keep_players: false,
  combat_turn_popup: true,
  combat_passive_initiative: false,
};

// ── library (crates/store/src/assets.rs) ──

export type AssetKind = 'token' | 'cutout' | 'map';
export const ASSET_KINDS: AssetKind[] = ['token', 'cutout', 'map'];
export const KIND_LABELS: Record<AssetKind, string> = { token: 'Token', cutout: 'Cut-out', map: 'Map' };

export type SizeLabel = 'Tiny' | 'Small' | 'Medium' | 'Large' | 'Huge' | 'Gargantuan';
export const SIZE_LABELS: SizeLabel[] = ['Tiny', 'Small', 'Medium', 'Large', 'Huge', 'Gargantuan'];
export const SIZE_INCHES: Record<SizeLabel, number> = {
  Tiny: 0.5, Small: 0.8, Medium: 1, Large: 2, Huge: 3, Gargantuan: 4,
};

export interface Asset {
  id: number;
  kind: string;
  name: string;
  aliases: string;
  size_label: string;
  pinned: boolean;
  original_blob: string;
  rendered_blob: string;
  pdf_blob: string | null;
  thumb_blob: string | null;
  preview_blob: string | null;
  params_json: string | null;
  legacy_id: number | null;
  created: string;
  updated: string;
  tags: string[];
  campaign_ids: string[];
  session_ids: string[];
}

export type AssetSort = 'newest' | 'oldest' | 'name_asc' | 'name_desc' | 'kind' | 'size';

export interface AssetQuery {
  q?: string | null;
  kinds?: string[];
  tags?: string[];
  pinned_only?: boolean;
  sort?: AssetSort;
  limit?: number | null;
  offset?: number | null;
}

export interface AssetPage {
  items: Asset[];
  total: number;
}

export interface AssetPatch {
  name?: string | null;
  aliases?: string | null;
  size_label?: string | null;
  pinned?: boolean | null;
}

export interface ImportFile {
  path: string;
  name?: string | null;
  kind: AssetKind;
  size?: string | null;
}

export interface ImportReport {
  imported: number[];
  failed: { path: string; error: string }[];
}

export type ExportWhat = 'original' | 'rendered' | 'pdf';

export interface TagCount {
  id: number;
  name: string;
  count: number;
}

export interface RenameOutcome {
  id: number;
  merged_into: number | null;
}

// ── tokens (crates/core/src/token.rs, crates/service/src/tokens.rs, token_sheet.rs) ──

export interface TokenParams {
  border: string;
  bg: [number, number, number] | null;
  ring_inset_pct: number;
  pan: [number, number];
  zoom: number;
  /** Map filters on the image only (TOKEN_FILTERS); the ring is never filtered. */
  filters: FilterStep[];
}

/** Token outline (crates/core/src/token.rs `TokenShape`): `hex` is pointy-top,
 *  `octagon` has flat top/bottom/sides, `decagon` a flat top and bottom. */
export type TokenShape = 'circle' | 'square' | 'hex' | 'octagon' | 'decagon';

export interface Border {
  id: string;
  name: string;
  builtin: boolean;
  blob: string;
  width: number;
  height: number;
  shape: TokenShape;
  /** A blank border: the image is only clipped to the shape. */
  borderless: boolean;
}

export interface SourceImage {
  hash: string;
  width: number;
  height: number;
}

export interface TokenDraft {
  source: SourceImage;
  params: TokenParams;
  name: string;
  size: string;
  asset_id: number | null;
}

export interface SheetItem {
  id: string;
  label: string;
  size: string;
  copies: number;
  image: string;
  thumb: string;
  asset_id: number | null;
  /** Outline for the cut line, detected from the image when added. */
  shape: TokenShape;
}

export interface TokenSheet {
  items: SheetItem[];
  format: PageFormat;
  cut_border: boolean;
}

export interface PlacedItem {
  id: string;
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface SheetLayout {
  page_w: number;
  page_h: number;
  pages: PlacedItem[][];
  skipped: string[];
}

export interface SheetItemPatch {
  label?: string | null;
  size?: string | null;
  copies?: number | null;
}

// ── cut-outs (crates/core/src/cutout.rs, crates/service/src/cutouts.rs, object_sheet.rs) ──

export type LassoMode = 'polygon' | 'freehand';

export interface CutoutParams {
  mode: LassoMode;
  points: [number, number][];
  outline_rgb: [number, number, number];
  outline_px: number;
}

export interface CutoutDraft {
  source: SourceImage;
  params: CutoutParams;
  name: string;
  size: string;
  asset_id: number | null;
}

export type SheetMode = 'flat' | 'standee';

export interface ObjectItem {
  id: string;
  label: string;
  size: string;
  scale: number;
  copies: number;
  image: string;
  thumb: string;
  w_px: number;
  h_px: number;
  asset_id: number | null;
}

export interface ObjectSheet {
  items: ObjectItem[];
  format: PageFormat;
  mode: SheetMode;
}

export interface ObjectItemPatch {
  label?: string | null;
  size?: string | null;
  scale?: number | null;
  copies?: number | null;
}

// ── cards (crates/core/src/cards.rs, crates/service/src/cards.rs) ──

export type Flip = 'long' | 'short';
export type FontStyleName = 'regular' | 'bold' | 'italic' | 'bold_italic';
export type Rgb = [number, number, number];

export interface CardOptions {
  format: PageFormat;
  double_sided: boolean;
  flip: Flip;
}

export type CardOp =
  | { op: 'rect'; x: number; y: number; w: number; h: number; width: number; rgb: Rgb }
  | { op: 'fill'; x: number; y: number; w: number; h: number; rgb: Rgb }
  | { op: 'line'; x1: number; y1: number; x2: number; y2: number; width: number; rgb: Rgb; dash: [number, number] | null }
  | { op: 'text'; x: number; y: number; text: string; style: FontStyleName; size: number; rgb: Rgb };

export interface DeckLayout {
  page_w: number;
  page_h: number;
  margin_x: number;
  margin_y: number;
  pages: { back: boolean; ops: CardOp[] }[];
  cards: number;
  overflow: string[];
  backs: boolean;
}

export interface DeckSummary {
  id: number;
  name: string;
  updated: string;
}

export interface Deck extends DeckSummary {
  markdown: string;
  settings_json: string;
  created: string;
}

export interface CardTemplates {
  templates: Record<string, string>;
  reference: string;
}

// ── Maps (Map Paginator) ── mirrors crates/service/src/maps.rs + core::maps

export type SizingMode = 'rows_cols' | 'reference' | 'ppi' | 'cell_side';
export type GridStyle = 'solid' | 'dashed' | 'dotted' | 'broken';
export type Pt = [number, number];

export interface FilterStep {
  type: string;
  enabled: boolean;
  params: Record<string, number>;
}

export interface FilterParamDef {
  key: string;
  label: string;
  min: number;
  max: number;
  default: number;
  step: number;
}

export interface FilterDef {
  key: string;
  label: string;
  params: FilterParamDef[];
}

export interface FilterPreset {
  name: string;
  pipeline: FilterStep[];
}

export interface FilterCatalog {
  defs: FilterDef[];
  presets: FilterPreset[];
}

export interface MapDoc {
  source: SourceImage | null;
  pre_warp: SourceImage | null;
  name: string;
  asset_id: number | null;
  detected_dpi: number;
  dpi_detected: boolean;
  mode: SizingMode;
  rows: number;
  cols: number;
  square_cells: boolean;
  ref_n: number;
  ref_quad: [Pt, Pt, Pt, Pt] | null;
  ppi: number;
  side: number;
  offset_x: number;
  offset_y: number;
  color: Rgb8;
  thickness: number;
  opacity: number;
  visible: boolean;
  style: GridStyle;
  remove_grid: boolean;
  filters: FilterStep[];
  format: PageFormat;
  /** Pages turned sideways. */
  landscape: boolean;
  square_in: number;
  calibration_mm: number;
  margin_in: number;
  overlap_in: number;
  align_to_grid: boolean;
  cut_border: boolean;
  fit_w_ft: number;
  fit_h_ft: number;
  /** [x, y, w, h] in full-res image px, or null for the whole map. */
  region: [number, number, number, number] | null;
  page_offset: [number, number];
  /** Excluded pages as [col, row]. */
  excluded: [number, number][];
}

export type Rgb8 = [number, number, number];

export interface GridModel {
  cell_w: number;
  cell_h: number;
  offset_x: number;
  offset_y: number;
  color: Rgb8;
  thickness: number;
  opacity: number;
  visible: boolean;
  style: GridStyle;
}

export interface Tile {
  col: number;
  row: number;
  /** [l, t, r, b] image px */
  crop: [number, number, number, number];
  page_rect: [number, number, number, number];
  page_x_pt: number;
  page_y_pt: number;
  w_pt: number;
  h_pt: number;
}

export interface FitCheck {
  squares: [number, number];
  printed_in: [number, number];
  fits: boolean;
}

export interface MapLayout {
  grid: GridModel;
  base_offset: [number, number];
  work_scale: number;
  proxy_w: number;
  proxy_h: number;
  scale_x: number;
  scale_y: number;
  tiles: Tile[];
  cols: number;
  rows: number;
  selected: number;
  fit: FitCheck | null;
  summary: string;
  page_w_pt: number;
  page_h_pt: number;
  margin_pt: number;
  /** A layout with fewer pages, when one exists (`map_fewest_pages` applies it). */
  fewest: FewestPages | null;
}

/** core::maps::tiling::FewestPages */
export interface FewestPages {
  landscape: boolean;
  offset: [number, number];
  pages: number;
  cols: number;
  rows: number;
}

// ── AoE templates ── mirrors core::aoe + crates/service/src/aoe.rs

export type AoeShape = 'circle' | 'cone' | 'line' | 'square';

export interface AoeConfig {
  shape: AoeShape;
  a_ft: number;
  b_ft: number;
  feet_per_square: number;
  fill_rgb: Rgb8;
  fill_alpha: number;
  line_rgb: Rgb8;
  line_width: number;
  grid_on: boolean;
  grid_rgb: Rgb8;
  grid_alpha: number;
  label_on: boolean;
  label: string;
  inch: number;
  page_format: PageFormat;
  name: string;
}

export interface AoeInfo {
  footprint_ft: [number, number];
  inches: [number, number];
  image_w: number;
  image_h: number;
  cell_px: number;
  pages: number;
  cols: number;
  rows: number;
  page_rects: [number, number, number, number][];
  label: string;
  summary: string;
}

export interface AoePreset {
  name: string;
  builtin: boolean;
  config: AoeConfig;
}

export interface AoeSheetItem {
  id: string;
  config: AoeConfig;
}

export interface AoeSheet {
  items: AoeSheetItem[];
  gap_in: number;
}

export interface AoePlaced {
  id: string;
  page: number;
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface AoeSheetLayout {
  page_w: number;
  page_h: number;
  pages: number;
  placed: AoePlaced[];
  skipped: string[];
  summary: string;
}

// ── speech to text (docs/features/transcribe.md) ──

export interface SttModel {
  id: string;
  label: string;
  hint: string;
  /** bytes */
  size: number;
  installed: boolean;
  /** seconds of work per second of audio (beam 5) on this machine */
  speed: number;
  /** measured here, else the built-in guess */
  measured: boolean;
}

export interface CloudModelSpec {
  id: string;
  label: string;
  usd_per_min: number;
}

export interface EngineEstimate {
  model: string;
  /** null when the duration is unknown */
  seconds: number | null;
  measured: boolean;
  /** cloud, when the price is known: USD */
  cost: number | null;
}

export interface TranscribeEstimate {
  duration_ms: number | null;
  local: EngineEstimate[];
  cloud: EngineEstimate;
}

export interface Correction {
  from: string;
  to: string;
}

export interface TranscribeOptions {
  /** default: the stt_engine setting */
  engine?: SttEngine | null;
  /** local: a catalog id; cloud: an OpenRouter model id (default: the stt_cloud_model setting) */
  model?: string | null;
  /** 'it' | 'en' | 'auto' | any ISO 639-1 code */
  language?: string | null;
  glossary?: string[];
  corrections?: Correction[];
  /** 1 = greedy (faster) */
  beam_size?: number | null;
}

export interface Segment {
  start_ms: number;
  end_ms: number;
  text: string;
}

export interface TranscribeResult {
  engine: SttEngine;
  model: string;
  language: string;
  duration_ms: number;
  segments: Segment[];
  /** cloud only: USD reported by the API */
  cost: number | null;
}

/** Never contains the key itself. */
export interface CloudKeyStatus {
  configured: boolean;
  source: 'saved' | 'env' | null;
  /** e.g. sk-or-…a1b2 */
  hint: string | null;
}

export interface CloudKeyInfo {
  label: string | null;
  /** USD; null = unlimited */
  limit: number | null;
  usage: number | null;
}


export type TranscriptFormat = 'txt' | 'md' | 'srt' | 'vtt';

/** What the save-to-library dialog returns (SaveAssetDialog.svelte). */
export interface SaveDetails {
  name: string;
  /** print size label; ignored for maps */
  size: string;
  tags: string[];
  /** overwrite the library asset the item was opened from */
  update: boolean;
}

// ── combat tracker (crates/core/src/combat) ──

export type CombatSort = 'ALPHA' | 'NUMBER';
export type CombatDir = 'ASC' | 'DESC';

export interface CombatCondition {
  id: string;
  name: string;
  color: string;
  /** Turns left; null is unlimited. */
  turns: number | null;
  /** Turns spent under it so far. */
  elapsed: number;
}

/** A stat cell: the column id plus value, or current/max, or tokenUrl/imageHref. */
export interface CombatCell {
  id: string;
  value?: string | number | boolean | null;
  current?: number | null;
  max?: number | null;
  tokenUrl?: string | null;
  imageHref?: string | null;
  isEditable?: boolean;
}

export interface CombatRow {
  id: string;
  name: string;
  display_name: string | null;
  custom_name: string | null;
  source: string | null;
  scaled_cr: number | null;
  scaled_summon_spell_level: number | null;
  scaled_summon_class_level: number | null;
  hp_current: number | null;
  hp_max: number | null;
  /** Temporary hit points. */
  hp_temp: number | null;
  initiative: number | null;
  is_active: boolean;
  conditions: CombatCondition[];
  notes: string;
  is_player_visible: boolean;
  ordinal: number;
  cells: CombatCell[];
  /** Stat block fields changed for this creature (5etools keys: ac, speed, str, …). */
  overrides: Record<string, unknown> | null;
  /** Creatures: which side they fight on (null = enemy); 'lair' marks a lair row. */
  side: 'ally' | 'neutral' | 'lair' | null;
  /** Play-time bookkeeping: spent uses/recharges by ability name, __la, __ds, __lair. */
  usage: Record<string, unknown> | null;
}

export interface CombatColumn {
  id: string;
  is_editable: boolean;
  /** 0 nobody, 1 everyone, 2 player rows only */
  is_player_visible: number;
  populate_with: string;
  abbreviation: string;
}

export interface CombatCustomCondition {
  id: string;
  name: string;
  color: string;
  turns: number | null;
}

export interface Tracker {
  sort: CombatSort;
  dir: CombatDir;
  is_roll_init: boolean;
  is_roll_hp: boolean;
  is_roll_groups: boolean;
  is_reroll_initiative_each_round: boolean;
  /** Passive initiative (10 + bonus) instead of rolling. */
  is_passive_init: boolean;
  is_invert_wound_direction: boolean;
  import_is_roll_groups: boolean;
  import_is_add_players: boolean;
  import_is_append: boolean;
  is_stats_add_columns: boolean;
  player_init_show_exact_player_hp: boolean;
  player_init_show_exact_monster_hp: boolean;
  player_init_hide_new_monster: boolean;
  player_init_show_ordinals: boolean;
  is_notes_shown: boolean;
  stats_cols: CombatColumn[];
  conditions_custom: CombatCustomCondition[];
  rows: CombatRow[];
  rows_default_party: CombatRow[];
  /** Defeated creatures set aside (out of the turn order). */
  defeated: CombatRow[];
  round: number;
  notes: string;
  is_locked: boolean;
  extra: Record<string, unknown>;
}

export interface ColumnKind {
  populate_with: string;
  abbreviation: string;
  name: string;
}

export interface CombatCatalog {
  /** [name, colour] */
  conditions: [string, string][];
  columns: ColumnKind[];
}

export interface BestiaryEntry {
  hash: string;
  name: string;
  source: string;
  cr: string | null;
  hp: number | null;
  ac: unknown;
  token_url: string | null;
  /** Tiny … Gargantuan. */
  size: string;
}

export interface BestiaryStatus {
  base_url: string;
  synced: boolean;
  files: number;
  creatures: number;
}

export interface EncounterImport {
  rows_added: number;
  missing: string[];
  unscaled: string[];
}

export interface CombatImport {
  kind: 'combat' | 'encounter';
  tracker: Tracker;
  encounter: EncounterImport | null;
}

export interface RowPatch {
  name?: string;
  custom_name?: string;
  hp_current?: number;
  hp_max?: number;
  hp_temp?: number;
  damage?: number;
  initiative?: number;
  notes?: string;
  remove_conditions?: string[];
  /** 5etools stat fields to change for this creature only (null removes one). */
  stats?: Record<string, unknown>;
  reset_stats?: boolean;
}

export interface EncounterCreature {
  hash: string;
  count: number;
  /** 5etools stat fields changed for these copies. */
  stats?: Record<string, unknown> | null;
}

export interface EncounterLine {
  hash: string;
  name: string;
  source: string;
  count: number;
  cr: string | null;
  /** The bestiary's CR when the line is scaled. */
  base_cr: string | null;
  known: boolean;
}

export interface EncounterSummary {
  id: string;
  name: string;
  notes: string;
  kind: 'encounter' | 'combat';
  creatures: EncounterCreature[];
  combat: unknown | null;
  pinned: boolean;
  created: string;
  updated: string;
  lines: EncounterLine[];
  total: number;
  xp: number | null;
}

export interface Hero {
  id: string;
  name: string;
  species: string;
  class: string;
  subclass: string;
  level: number | null;
  hp_max: number | null;
  ac: number | null;
  initiative_bonus: number | null;
  passive_perception: number | null;
  speed: number | null;
  spell_dc: number | null;
  player: string;
  notes: string;
}

export interface Party {
  id: string;
  name: string;
  notes: string;
  heroes: Hero[];
  updated: string;
}
