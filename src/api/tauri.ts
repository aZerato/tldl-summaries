import { invoke } from "@tauri-apps/api/core";
import type {
  AppConfig,
  DeviceInfo,
  HistoryRecord,
  MediaInfo,
  ModelInfo,
  PipelineOutput,
  PromptContext,
} from "../types";

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

/** Devices de calcul disponibles (F10) — CPU + GPU/iGPU détectés. */
export const listDevices = () => invoke<DeviceInfo[]>("list_devices");

export const getConfig = () => invoke<AppConfig>("get_config");

export const setConfig = (config: AppConfig) =>
  invoke<null>("set_config", { config });

export const listContexts = () =>
  invoke<PromptContext[]>("list_contexts");

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
