export interface PoolCredentialRefs {
  username_ref: string;
  password_ref: string;
}

export interface PoolsConfig {
  bpmsupreme: PoolCredentialRefs;
  clubkillers: PoolCredentialRefs;
  livedjservice: PoolCredentialRefs;
  qobuz: PoolCredentialRefs;
}

export type SearchMode = 'best_match' | 'show_results';
export type SearchPriority = 'pools' | 'soundcloud' | 'youtube' | 'qobuz';
export type KeyNotation = 'musical' | 'camelot';

export interface Config {
  output_dir: string;
  auto_download: boolean;
  search_mode: SearchMode;
  search_priority: SearchPriority;
  pools: PoolsConfig;
  bpm_range_min?: number;
  bpm_range_max?: number;
  key_notation: KeyNotation;
}

export type DownloadStatus = 'queued' | 'downloading' | 'analyzing' | 'done' | 'error';

export interface DownloadItem {
  id: string;
  input: string;
  track_name?: string;
  file_path?: string;
  source?: string;
  bpm?: number;
  key?: string;
  progress?: number;
  status: DownloadStatus;
  error?: string;
}

// IPC event payloads
export interface ProgressPayload { id: string; percent: number }
export interface DonePayload { id: string; file_path: string; track_name: string; source: string }
export interface ErrorPayload { id: string; message: string }
export interface AnalysisDonePayload { id: string; bpm: number; key: string }
export interface SearchResultItem { title: string; url: string; source: string }
export interface SearchResultsPayload { id: string; items: SearchResultItem[] }
