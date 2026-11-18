import { invoke } from "@tauri-apps/api/core";
import type {
  AppConfig,
  HistoryRecord,
  MediaInfo,
  ModelInfo,
  PipelineOutput,
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
) =>
  invoke<PipelineOutput>("run_pipeline", {
    input,
    asrModel,
    llmModel,
    language,
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

export const getConfig = () => invoke<AppConfig>("get_config");

export const setConfig = (config: AppConfig) =>
  invoke<null>("set_config", { config });
