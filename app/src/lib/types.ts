// Mirrors of the Rust types serialized by the service layer.
// Keep in sync with crates/core/src/settings.rs and crates/service/src/*.rs.

export type PageFormat = 'Letter' | 'A4';
export type CutLineStyle = 'Dashed' | 'Dotted' | 'Solid';
export type Theme = 'system' | 'light' | 'dark';

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
  | { type: 'store_changed'; entity: string; entity_id: string; op: string };

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
  campaign_id?: string | null;
  session_id?: string | null;
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

export interface Campaign {
  id: string;
  name: string;
  created: string;
  session_count: number;
  asset_count: number;
}

export interface Session {
  id: string;
  campaign_id: string;
  name: string;
  notes: string;
  created: string;
  asset_count: number;
}
