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
  /** Device utilisé (F10) : ex. « CPU », « NVIDIA … ». */
  device: string;
  /** Note de bascule (F10) : GPU demandé mais non détecté (exécution CPU). */
  deviceNote: string | null;
  /** Chemin du fichier source (F13) — lecteur audio de la page résultat. */
  file: string;
  durationSecs: number;
}

/** Device de calcul ggml (F10). */
export interface DeviceInfo {
  name: string;
  /** "cpu" | "gpu" | "igpu" | "accel". */
  kind: string;
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
  /** Horodatage Unix (secondes) — à formater côté UI. */
  created: number;
  durationSecs: number;
  asrModel: string;
  llmModel: string;
  /** Device de calcul utilisé (F10) ; vide avant la migration. */
  device: string;
  /** Nom du contexte de prompt (F15) ; vide avant la migration. */
  contextName: string;
  transcription: string;
  /** JSON complet du `Transcription` (texte + segments + langue) ;
   *  `null` sur les lignes écrites avant la migration. */
  transcriptionJson: string | null;
  summaryMarkdown: string;
}

/** Contexte de prompt (F15) — instructions de **résumé** réutilisables :
 *  n'agit que sur l'étape LLM, jamais sur la transcription. */
/** Vue frontend : métadonnées de l'enregistrement affiché (page Résultat) —
 *  le retour Rust `PipelineOutput` n'en porte pas. */
export interface ResultMeta {
  /** Nom du contexte de prompt (F15) ; vide = lignes pré-migration. */
  contextName: string;
  /** Horodatage Unix (secondes). */
  created: number;
}

export interface PromptContext {
  id: number;
  name: string;
  instructions: string;
  isFavorite: boolean;
  /** Horodatage Unix (secondes) — à formater côté UI. */
  created: number;
}

export interface AppConfig {
  profile: "leger" | "equilibre" | "puissant";
  asrModel: string;
  llmModel: string;
  language: string;
  temperature: number;
  useGpu: boolean;
}
