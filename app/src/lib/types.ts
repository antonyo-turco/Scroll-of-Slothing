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
