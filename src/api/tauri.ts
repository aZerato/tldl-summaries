import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import type {
  AppConfig,
  DeviceInfo,
  Folder,
  HistoryRecord,
  MediaInfo,
  ModelInfo,
  PipelineOutput,
  PromptContext,
  Transcription,
} from "../types";

// F13 : convertit un chemin système en URL `asset://` servie par le
// protocole asset (tauri.conf.json : `app.security.assetProtocol`) —
// lecture du média source dans le lecteur de la page résultat.
export { convertFileSrc };

// Tauri 2 convertit les arguments des commandes (camelCase JS → snake_case
// Rust) ; les VALEURS DE RETOUR ne sont pas converties : elles suivent les
// attributs serde des structs Rust (`rename_all = "camelCase"`, cf. types.ts).
export const probeMedia = (path: string) =>
  invoke<MediaInfo>("probe_media", { path });

export const runPipeline = (
  input: string,
  asrModel: string,
  llmModel: string,
  language: string,
  /** Id du contexte de prompt (F15) ; `null` = contexte par défaut. */
  contextId: number | null,
  /** F14 : section traitée (s, bornes absolues du fichier source) ;
   *  `null` = fichier complet. */
  sectionStart: number | null,
  /** F14 : fin de section ; `null` ou `0` = jusqu'à la fin du fichier. */
  sectionEnd: number | null,
) =>
  invoke<PipelineOutput>("run_pipeline", {
    input,
    asrModel,
    llmModel,
    language,
    contextId: contextId === null ? null : contextId,
    sectionStart: sectionStart === null ? null : sectionStart,
    sectionEnd: sectionEnd === null ? null : sectionEnd,
  });

export const listModels = () => invoke<ModelInfo[]>("list_models");

export const downloadModel = (id: string) =>
  invoke<null>("download_model", { id });

export const cancelModelDownload = (id: string) =>
  invoke<boolean>("cancel_model_download", { id });

export const deleteModel = (id: string) => invoke<null>("delete_model", { id });

export const listHistory = (limit = 50) =>
  invoke<HistoryRecord[]>("list_history", { limit });

export const getHistory = (id: number) =>
  invoke<HistoryRecord>("get_history", { id });

export const searchHistory = (query: string) =>
  invoke<HistoryRecord[]>("search_history", { query });

/** Résultats déjà produits pour un fichier (F16) — plus récents en premier. */
export const historyForFile = (file: string) =>
  invoke<HistoryRecord[]>("history_for_file", { file });

/** Re-résumé d'un enregistrement **sans re-transcrire** (F15/F16) : seule
 *  l'étape LLM est relancée (contexte choisi), la transcription stockée est
 *  réutilisée. Renvoie le nouvel enregistrement consigné. */
export const reSummarize = (id: number, contextId: number | null) =>
  invoke<HistoryRecord>("re_summarize", {
    id,
    contextId: contextId === null ? null : contextId,
  });

/** Supprime un enregistrement de l'historique (F7). */
export const deleteHistory = (id: number) =>
  invoke<null>("delete_history", { id });

/** Crée un dossier de rangement (F19) ; renvoie l'id. */
export const createFolder = (name: string) =>
  invoke<number>("create_folder", { name });

/** Dossiers de rangement (F19) + compte d'enregistrements de chacun. */
export const listFolders = () => invoke<Folder[]>("list_folders");

/** Supprime un dossier (F19) ; ses résultats repassent « Sans dossier ». */
export const deleteFolder = (id: number) => invoke<null>("delete_folder", { id });

/** Rangement (F19) : `folderId = null` = « Sans dossier ». */
export const moveRecord = (id: number, folderId: number | null) =>
  invoke<null>("move_record", {
    id,
    folderId: folderId === null ? null : folderId,
  });

/** Rangement de groupe (F19) : tous les résumés d'une même transcription
 *  (un fichier) vers un dossier, ou « Sans dossier » — atomique côté Rust. */
export const moveFileToFolder = (file: string, folderId: number | null) =>
  invoke<number>("move_file_to_folder", {
    file,
    folder: folderId === null ? null : folderId,
  });

/** Enregistrements rangés dans un dossier (F19) — `null` = « Sans dossier ». */
export const listHistoryFolder = (folderId: number | null, limit = 50) =>
  invoke<HistoryRecord[]>("list_history_folder", {
    folder: folderId === null ? null : folderId,
    limit,
  });

/** Devices de calcul disponibles (F10) — CPU + GPU/iGPU détectés. */
export const listDevices = () => invoke<DeviceInfo[]>("list_devices");

export const getConfig = () => invoke<AppConfig>("get_config");

export const setConfig = (config: AppConfig) =>
  invoke<null>("set_config", { config });

export const listContexts = () =>
  invoke<PromptContext[]>("list_contexts");

/** Reconstruit le résultat affiché à partir d'un enregistrement (F7).
 *  Utilisé par l'historique (F7) et la redirection F16. */
export function toOutput(r: HistoryRecord): PipelineOutput {
  let transcription: Transcription;
  if (r.transcriptionJson) {
    try {
      transcription = JSON.parse(r.transcriptionJson) as Transcription;
    } catch {
      // JSON illisible : repli sur le texte seul, sans segments.
      transcription = { text: r.transcription, segments: [], language: "" };
    }
  } else {
    // Ligne écrite avant la migration : texte seul, pas de segments.
    transcription = { text: r.transcription, segments: [], language: "" };
  }
  return {
    transcription,
    summaryMarkdown: r.summaryMarkdown,
    summaryError: null,
    asrModel: r.asrModel,
    llmModel: r.llmModel,
    device: r.device,
    deviceNote: null,
    file: r.file,
    durationSecs: r.durationSecs,
    sectionStart: r.sectionStart,
    sectionEnd: r.sectionEnd,
  };
}

export const saveContext = (
  id: number | null,
  name: string,
  instructions: string,
) =>
  invoke<number>("save_context", {
    id: id === null ? null : id,
    name,
    instructions,
  });

export const deleteContext = (id: number) =>
  invoke<null>("delete_context", { id });

export const setContextFavorite = (id: number, favorite: boolean) =>
  invoke<null>("set_context_favorite", { id, favorite });
