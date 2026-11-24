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
  /** F9 : intervenante du segment (« Intervenante 1 »…) ; `undefined` si
   *  pas de diarisation (désactivée ou modèle absent). */
  speaker?: string;
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
  /** F14 : début de la section traitée, dans le fichier source (s) ;
   *  `0` si le fichier complet a été traité. */
  sectionStart: number;
  /** F14 : fin de la section traitée, dans le fichier source (s) ;
   *  `= durationSecs` si le fichier complet a été traité. */
  sectionEnd: number;
  /** F9 : note de dégradation de la diarisation — `null` si les
   *  intervenants sont présents ou la séparation est désactivée. */
  diarError: string | null;
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
  kind: "asr" | "llm" | "diar";
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
  /** Dossier de rangement (F19) ; `null` = « Sans dossier ». */
  folderId: number | null;
  /** F14 : début de la section traitée, dans le fichier source (s) ;
   *  `0` si le fichier complet a été traité. */
  sectionStart: number;
  /** F14 : fin de la section traitée, dans le fichier source (s). */
  sectionEnd: number;
}

/** Dossier de rangement (F19) : les résultats sont rangés dans au plus un
 *  dossier ; « Sans dossier » est l'état par défaut (jamais supprimé par
 *  la suppression d'un dossier). */
export interface Folder {
  id: number;
  name: string;
  /** Horodatage Unix (secondes) — à formater côté UI. */
  created: number;
  /** Nombre d'enregistrements rangés dans ce dossier. */
  count: number;
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
  /** F9 : séparation des intervenants (diarisation) activée. Le nombre
   *  d'intervenants (auto / N) n'est pas persisté : il est demandé au
   *  moment de la transcription (argument de `run_pipeline`). */
  diarize: boolean;
}
