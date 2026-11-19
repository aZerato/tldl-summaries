import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import type {
  AppConfig,
  DeviceInfo,
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
) =>
  invoke<PipelineOutput>("run_pipeline", {
    input,
    asrModel,
    llmModel,
    language,
    contextId: contextId === null ? null : contextId,
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
