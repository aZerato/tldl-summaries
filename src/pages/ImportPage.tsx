import { useEffect, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { probeMedia, runPipeline } from "../api/tauri";
import Progress from "../components/Progress";
import type {
  AppConfig,
  MediaInfo,
  PipelineOutput,
  Progress as ProgressEvent,
} from "../types";

interface Props {
  config: AppConfig | null;
  onDone: (result: PipelineOutput) => void;
}

export default function ImportPage({ config, onDone }: Props) {
  const [path, setPath] = useState<string | null>(null);
  const [info, setInfo] = useState<MediaInfo | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [progress, setProgress] = useState<ProgressEvent | null>(null);

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

  async function pick(file: File) {
    // En webview Tauri, les fichiers exposent leur chemin ; sinon on renvoie
    // vers le sélecteur de fichiers.
    const p = (file as { path?: string }).path;
    if (!p) {
      setError(
        "Chemin du fichier non exposé — utilisez « Choisir un fichier ».",
      );
      return;
    }
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
      setPath(p);
      setInfo(null);
      setError(null);
      probeMedia(p)
        .then(setInfo)
        .catch((e) => setError(String(e)));
    }
  }

  async function run() {
    if (!path || !config) return;
    setBusy(true);
    setError(null);
    try {
      const r = await runPipeline(
        path,
        config.asr_model,
        config.llm_model,
        config.language,
      );
      onDone(r);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
      setProgress(null);
    }
  }

  return (
    <div className="page">
      <div
        className="drop"
        onDragOver={(e) => e.preventDefault()}
        onDrop={(e) => {
          e.preventDefault();
          const f = e.dataTransfer.files[0];
          if (f) pick(f);
        }}
      >
        <p className="drop-title">Glissez un fichier audio ou vidéo ici</p>
        <p className="muted">
          wav · mp3 · m4a · flac · ogg · opus — mp4 · mov · mkv · webm · avi
        </p>
        <input
          type="file"
          hidden
          onChange={(e) => {
            const f = e.target.files?.[0];
            if (f) pick(f);
          }}
        />
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
                  info.duration_secs,
                )}${info.has_video ? " · vidéo (convertie en audio)" : ""}`
              : "lecture du fichier…"}
          </div>
        </div>
      )}

      {progress && <Progress progress={progress} />}

      <div className="row">
        <button
          className="btn primary"
          disabled={!path || busy || info?.has_audio === false}
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
