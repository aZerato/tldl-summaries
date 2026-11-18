import { useEffect, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { listen } from "@tauri-apps/api/event";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import {
  listContexts,
  probeMedia,
  runPipeline,
  saveContext,
} from "../api/tauri";
import Progress from "../components/Progress";
import type {
  AppConfig,
  MediaInfo,
  PipelineOutput,
  Progress as ProgressEvent,
  PromptContext,
} from "../types";

interface Props {
  config: AppConfig | null;
  onDone: (result: PipelineOutput) => void;
  onGoContexts: () => void;
}

export default function ImportPage({ config, onDone, onGoContexts }: Props) {
  const [path, setPath] = useState<string | null>(null);
  const [info, setInfo] = useState<MediaInfo | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [progress, setProgress] = useState<ProgressEvent | null>(null);
  const [dragging, setDragging] = useState(false);
  // F15 : contexte de prompt (null = contexte « rencontre » par défaut).
  const [contexts, setContexts] = useState<PromptContext[]>([]);
  const [contextId, setContextId] = useState<number | null>(null);
  const [creating, setCreating] = useState(false);
  const [newName, setNewName] = useState("");
  const [newInstructions, setNewInstructions] = useState("");
  const [creatingBusy, setCreatingBusy] = useState(false);

  async function loadContexts() {
    try {
      const c = await listContexts();
      setContexts(c);
      // Un favori disparu (supprimé) ne doit pas rester sélectionné.
      setContextId((id) => (id === null || c.some((x) => x.id === id) ? id : null));
    } catch (e) {
      setError(String(e));
    }
  }

  useEffect(() => {
    loadContexts().catch(() => undefined);
  }, []);

  useEffect(() => {
    if (!isTauri) return;
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    listen<ProgressEvent>("pipeline-progress", (event) =>
      setProgress(event.payload),
    ).then((u) => {
      if (!cancelled) unlisten = u;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  // Tauri 2 : le drop de fichiers est intercepté par la couche native — les
  // événements DOM (dragover/drop) ne se déclenchent PAS dans le webview, et
  // les `File` sandboxés n'exposent pas de `.path`. On écoute donc
  // `onDragDropEvent`, qui fournit les chemins absolus des fichiers déposés.
  useEffect(() => {
    if (!isTauri) return;
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    getCurrentWebview()
      .onDragDropEvent((event) => {
        const t = event.payload.type;
        if (t === "drop") {
          const p = event.payload.paths[0];
          if (p) pickPath(p);
        } else if (t === "enter" || t === "over") {
          setDragging(true);
        } else {
          // "leave" : drag annulé ou sorti de la fenêtre.
          setDragging(false);
        }
      })
      .then((u) => {
        if (!cancelled) unlisten = u;
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  async function pickPath(p: string) {
    setPath(p);
    setInfo(null);
    setError(null);
    try {
      const i = await probeMedia(p);
      setInfo(i);
    } catch (e) {
      setError(String(e));
    }
  }

  async function choose() {
    const p = await openFileDialog({
      multiple: false,
      directory: false,
      filters: [
        {
          name: "Audio",
          extensions: ["wav", "mp3", "m4a", "flac", "ogg", "opus"],
        },
        {
          name: "Vidéo",
          extensions: ["mp4", "mov", "mkv", "webm", "avi"],
        },
      ],
    });
    if (typeof p === "string") {
      pickPath(p);
    }
  }

  // F15 : création rapide d'un contexte depuis l'import.
  async function createAndUse() {
    if (!newName.trim() || creatingBusy) return;
    setCreatingBusy(true);
    try {
      const id = await saveContext(null, newName.trim(), newInstructions.trim());
      setCreating(false);
      setNewName("");
      setNewInstructions("");
      await loadContexts();
      setContextId(id);
    } catch (e) {
      setError(String(e));
    } finally {
      setCreatingBusy(false);
    }
  }

  async function run() {
    if (!path || !config) return;
    setBusy(true);
    setError(null);
    try {
      const r = await runPipeline(
        path,
        config.asrModel,
        config.llmModel,
        config.language,
        contextId,
      );
      onDone(r);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
      setProgress(null);
    }
  }

  const selectedContext =
    contextId === null ? null : contexts.find((c) => c.id === contextId) ?? null;

  return (
    <div className="page">
      <div className={dragging ? "drop over" : "drop"}>
        <p className="drop-title">Glissez un fichier audio ou vidéo ici</p>
        <p className="muted">
          wav · mp3 · m4a · flac · ogg · opus — mp4 · mov · mkv · webm · avi
        </p>
        <div className="row" style={{ justifyContent: "center" }}>
          <button className="btn" onClick={choose}>
            Choisir un fichier…
          </button>
        </div>
      </div>

      {path && (
        <div className="file-card">
          <div className="file-name" title={path}>
            {path.split(/[\\/]/).pop()}
          </div>
          <div className="muted">
            {info
              ? `${info.format} · ${formatDuration(
                  info.durationSecs,
                )}${info.hasVideo ? " · vidéo (convertie en audio)" : ""}`
              : "lecture du fichier…"}
          </div>
        </div>
      )}

      {progress && <Progress progress={progress} />}

      {/* F15 : choix du contexte de prompt avant la synthèse. */}
      <div className="file-card">
        <div className="row" style={{ marginTop: 0 }}>
          <label className="field" style={{ margin: 0, flex: 1 }}>
            Contexte d'analyse / résumé
            <select
              value={contextId === null ? -1 : contextId}
              onChange={(e) => {
                const v = Number(e.target.value);
                setContextId(v < 0 ? null : v);
              }}
            >
              <option value={-1}>Contexte par défaut (rencontre)</option>
              {contexts.map((c) => (
                <option key={c.id} value={c.id}>
                  {c.isFavorite ? "★ " : ""}
                  {c.name}
                </option>
              ))}
            </select>
          </label>
          <button className="btn" onClick={() => setCreating(!creating)}>
            {creating ? "Fermer la création" : "Créer un contexte"}
          </button>
        </div>
        {creating && (
          <div className="row">
            <label className="field" style={{ margin: 0 }}>
              Nom
              <input
                type="text"
                value={newName}
                onChange={(e) => setNewName(e.target.value)}
                placeholder="ex. Rapport de contrôle qualité"
              />
            </label>
            <label className="field" style={{ margin: 0, flex: 1 }}>
              Instructions
              <textarea
                rows={3}
                value={newInstructions}
                onChange={(e) => setNewInstructions(e.target.value)}
                placeholder="Comment doit l'analyse orienter le résumé ?"
              />
            </label>
          </div>
        )}
        {creating && (
          <div className="row">
            <button
              className="btn primary"
              disabled={!newName.trim() || creatingBusy}
              onClick={createAndUse}
            >
              Créer et utiliser
            </button>
            <button className="btn" onClick={onGoContexts}>
              Gérer les contextes
            </button>
          </div>
        )}
        {!creating && (
          <div className="muted">
            {selectedContext
              ? `« ${selectedContext.name} » — ${selectedContext.instructions || "contexte par défaut"}`
              : "Aucun contexte : le prompt « rencontre » (Synthèse / Points clés / Décisions / Actions) est utilisé."}
            {"  "}
            <button className="link" onClick={onGoContexts}>
              Gérer les contextes
            </button>
          </div>
        )}
      </div>

      <div className="row">
        <button
          className="btn primary"
          disabled={!path || busy || info?.hasAudio === false}
          onClick={run}
        >
          {busy ? "Synthèse en cours…" : "Lancer la synthèse"}
        </button>
      </div>

      {error && <div className="error">{error}</div>}
      <p className="muted hint">
        Premier lancement : les modèles ASR/LLM doivent être en place
        (menu Modèles).
      </p>
    </div>
  );
}

function formatDuration(secs: number): string {
  const m = Math.floor(secs / 60);
  const s = Math.round(secs % 60);
  return m > 0 ? `${m} min ${s.toString().padStart(2, "0")} s` : `${s} s`;
}
