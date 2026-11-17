// Types partagés frontend ↔ noyau Rust (cf. src-tauri).
//
// Convention de fil : les types Rust portent `#[serde(rename_all =
// "camelCase")]` et Tauri 2 ne convertit que les ARGUMENTS des commandes
// (JS → Rust), jamais les valeurs de retour — qui arrivent donc avec leurs
// clés serde (camelCase). Les interfaces ci-dessous reflètent le JSON réel.

export interface MediaInfo {
  path: string;
  durationSecs: number;
  hasVideo: boolean;
  hasAudio: boolean;
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
  summaryMarkdown: string;
  /** Résumé indisponible (modèle LLM absent) — message explicatif. */
  summaryError: string | null;
  asrModel: string;
  llmModel: string;
  durationSecs: number;
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
  sizeMb: number;
  quant: string;
  ramGb: number;
  isDefault: boolean;
  available: boolean;
}

/// Progression d'un téléchargement de modèle (F11) — événement
/// `model-download-progress`.
export interface DownloadProgress {
  id: string;
  received: number;
  total: number;
}

export interface HistoryRecord {
  id: number;
  file: string;
  created: string;
  durationSecs: number;
  asrModel: string;
  llmModel: string;
  transcription: string;
  summaryMarkdown: string;
}

export interface AppConfig {
  profile: "leger" | "equilibre" | "puissant";
  asrModel: string;
  llmModel: string;
  language: string;
  temperature: number;
  useGpu: boolean;
}
