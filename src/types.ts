// Types partagés frontend ↔ noyau Rust (cf. src-tauri).

export interface MediaInfo {
  path: string;
  duration_secs: number;
  has_video: boolean;
  has_audio: boolean;
  format: string;
}

export interface Segment {
  start: number;
  end: number;
  text: string;
}

export interface Transcription {
  text: string;
  segments: Segment[];
  language: string;
}

export interface PipelineOutput {
  transcription: Transcription;
  summary_markdown: string;
  /** Résumé indisponible (modèle LLM absent) — message explicatif. */
  summary_error: string | null;
  asr_model: string;
  llm_model: string;
  duration_secs: number;
}

export interface Progress {
  step: string;
  position: number;
  total: number;
}

export interface ModelInfo {
  id: string;
  kind: "asr" | "llm";
  name: string;
  size_mb: number;
  quant: string;
  ram_gb: number;
  is_default: boolean;
  available: boolean;
}

export interface HistoryRecord {
  id: number;
  file: string;
  created: string;
  duration_secs: number;
  asr_model: string;
  llm_model: string;
  transcription: string;
  summary_markdown: string;
}

export interface AppConfig {
  profile: "leger" | "equilibre" | "puissant";
  asr_model: string;
  llm_model: string;
  language: string;
  temperature: number;
  use_gpu: boolean;
}
