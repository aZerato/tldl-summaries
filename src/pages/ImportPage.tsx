import { useEffect, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { listen } from "@tauri-apps/api/event";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import {
  convertFileSrc,
  historyForFile,
  listContexts,
  probeMedia,
  reSummarize,
  runPipeline,
  saveContext,
  toOutput,
} from "../api/tauri";
import Progress from "../components/Progress";
import type {
  AppConfig,
  HistoryRecord,
  MediaInfo,
  PipelineOutput,
  Progress as ProgressEvent,
  PromptContext,
} from "../types";

interface Props {
  config: AppConfig | null;
  /** F15/F16 : le résultat + l'enregistrement consigné (re-résumé et
   *  résumés par contexte depuis le résultat) ; `null` si la relance a
   *  échoué. */
  onDone: (result: PipelineOutput, rec: HistoryRecord | null) => void;
  onGoContexts: () => void;
  /** F16 : redirection vers un résultat existant du fichier choisi. */
  onOpenExisting: (rec: HistoryRecord) => void;
}

export default function ImportPage({
  config,
  onDone,
  onGoContexts,
  onOpenExisting,
}: Props) {
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
  // F16 : résultats existants du fichier choisi (null = aucun/ inconnus).
  const [existing, setExisting] = useState<HistoryRecord[] | null>(null);
  // F16 : l'utilisateur a choisi de forcer une nouvelle analyse.
  const [forced, setForced] = useState(false);
  // F14 : coupe début/fin (sliders) — `null` = position par défaut
  // (fichier complet). La section porte sur le **traitement** (ASR + LLM ne
  // voient que la section), pas seulement l'affichage.
  const [secStart, setSecStart] = useState<number | null>(null);
  const [secEnd, setSecEnd] = useState<number | null>(null);

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
    // F16 : l'état « forcée » ne vaut que pour le dernier fichier choisi.
    setExisting(null);
    setForced(false);
    // F14 : la coupe repart sur le fichier complet pour chaque nouveau fichier.
    setSecStart(null);
    setSecEnd(null);
    // F1 : validation de l'extension à l'import — un format non pris en
    // charge donne un message clair (pas de plant, pas de ffprobe sur un
    // non-média qui échouerait sans message lisible).
    if (!isSupportedExt(p)) {
      const e = extOf(p);
      setError(
        `Format non pris en charge${e ? ` (${e})` : ""}. Formats acceptés : ${SUPPORTED_EXT.join(", ")}.`,
      );
      return;
    }
    try {
      const i = await probeMedia(p);
      setInfo(i);
      const prev = await historyForFile(p);
      setExisting(prev.length > 0 ? prev : null);
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
        // F14 : section choisie (sliders) ; `null` = fichier complet.
        secStart,
        secEnd,
      );
      // F15/F16 : le pipeline vient de consigner un nouvel enregistrement —
      // c'est le plus récent pour le fichier (historique trié, plus récents
      // d'abord). `null` si la relance échoue : le re-résumé reste possible
      // via l'historique.
      let rec: HistoryRecord | null = null;
      try {
        const fresh = await historyForFile(path);
        rec = fresh.length > 0 ? fresh[0] : null;
      } catch {
        rec = null;
      }
      onDone(r, rec);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
      setProgress(null);
    }
  }

  // F15/F16 : re-résumé **sans re-transcrire** — la transcription stockée est
  // réutilisée, seule l'étape LLM est relancée (avec le contexte choisi).
  async function resume() {
    if (!existing || existing.length === 0 || busy) return;
    setBusy(true);
    setError(null);
    setProgress(null);
    try {
      const rec = await reSummarize(existing[0].id, contextId);
      const fresh = await historyForFile(path ?? "");
      setExisting(fresh.length > 0 ? fresh : null);
      onDone(toOutput(rec), rec);
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
              : isSupportedExt(path)
                ? "lecture du fichier…"
                : "format non pris en charge"}
          </div>
        </div>
      )}

      {/* F14 : pré-écoute du média source (protocole asset, cf. F13) +
          coupe début/fin par sliders — la coupe porte sur le **traitement**
          (ASR + LLM ne voient que la section) ; défaut = fichier complet. */}
      {info !== null && path !== null && (
        <div className="file-card section-card">
          <div className="file-name">Pré-écoute</div>
          <audio className="player" controls src={convertFileSrc(path)} />
          <div className="row">
            <label className="field" style={{ flex: 1, margin: 0 }}>
              Début
              <input
                type="range"
                min={0}
                max={info.durationSecs}
                step={0.1}
                value={secStart ?? 0}
                onChange={(e) => {
                  const v = Number(e.target.value);
                  // Ne pas dépasser la fin courante (clamped, pas d'inversion).
                  setSecStart(Math.min(v, secEnd ?? info.durationSecs));
                }}
              />
            </label>
            <label className="field" style={{ flex: 1, margin: 0 }}>
              Fin
              <input
                type="range"
                min={0}
                max={info.durationSecs}
                step={0.1}
                value={secEnd ?? info.durationSecs}
                onChange={(e) => {
                  const v = Number(e.target.value);
                  // Ne pas remonter sous le début courant.
                  setSecEnd(Math.max(v, secStart ?? 0));
                }}
              />
            </label>
          </div>
          <div className="row">
            <span className="muted">
              Section : {fmtTime(secStart ?? 0)} →{" "}
              {fmtTime(secEnd ?? info.durationSecs)} (
              {Math.max(
                0,
                (secEnd ?? info.durationSecs) - (secStart ?? 0),
              ).toFixed(1)}{" "}
              s traitées)
            </span>
            <button
              className="btn"
              disabled={secStart === null && secEnd === null}
              onClick={() => {
                setSecStart(null);
                setSecEnd(null);
              }}
            >
              Tout le fichier
            </button>
          </div>
        </div>
      )}

      {/* F16 : fichier déjà analysé — avertir, forcer ou rediriger. */}
      {existing !== null && existing.length > 0 && (
        <div className="file-card">
          <div className="file-name">
            Ce fichier a déjà été analysé
            {existing.length > 1 ? ` (${existing.length} résultats)` : ""}
          </div>
          <div className="muted">
            Plus récent :{" "}
            {new Date(existing[0].created * 1000).toLocaleString()} · ASR{" "}
            {existing[0].asrModel} · LLM {existing[0].llmModel}
            {existing[0].contextName ? ` · ${existing[0].contextName}` : ""}
          </div>
          {forced ? (
            <div className="muted">
              Nouvelle analyse forcée — transcription + résumé recalculés ;
              les résultats existants restent dans l'historique.
            </div>
          ) : (
            <>
              <div className="row" style={{ flexWrap: "wrap" }}>
                <button
                  className="btn primary"
                  disabled={busy}
                  onClick={resume}
                >
                  {busy ? "En cours…" : "Résumer à nouveau sans re-transcrire"}
                </button>
                <button
                  className="btn"
                  onClick={() => onOpenExisting(existing[0])}
                >
                  Ouvrir le résultat existant
                </button>
                <button className="btn" onClick={() => setForced(true)}>
                  Forcer une nouvelle analyse
                </button>
              </div>
              <div className="muted" style={{ marginTop: 8 }}>
                Le résumé seul est recalculé (LLM) avec le contexte choisi
                ci-dessous ; la transcription stockée est réutilisée, pas de
                re-transcription.
              </div>
            </>
          )}
        </div>
      )}

      {progress && <Progress progress={progress} />}

      {/* F15 : choix du contexte de prompt avant la synthèse. */}
      <div className="file-card">
        <div className="row" style={{ marginTop: 0 }}>
          <label className="field" style={{ margin: 0, flex: 1 }}>
            Contexte de résumé
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
                placeholder="Instructions pour le résumé — la transcription reste identique"
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
            <div>
              Le contexte n'agit que sur le **résumé** (LLM) — la
              transcription reste fidèle à l'audio.
            </div>
          </div>
        )}
      </div>

      <div className="row">
        <button
          className="btn primary"
          disabled={
            !path ||
            !isSupportedExt(path) ||
            busy ||
            info?.hasAudio === false ||
            // F16 : un résultat existe — trancher d'abord (re-résumer sans
            // re-transcrire / rediriger / forcer) ; la synthèse complète
            // (avec re-transcription) ne se lance qu'une fois forcée.
            (existing !== null && existing.length > 0 && !forced)
          }
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

// F1 : les 11 extensions acceptées (audio + vidéo) — miroir des filtres du
// sélecteur et du critère « les 11 extensions ci-dessus acceptées ».
const SUPPORTED_EXT = [
  "wav",
  "mp3",
  "m4a",
  "flac",
  "ogg",
  "opus",
  "mp4",
  "mov",
  "mkv",
  "webm",
  "avi",
];
function extOf(p: string): string {
  const m = p.toLowerCase().match(/\.([a-z0-9]+)$/);
  return m ? m[1] : "";
}
function isSupportedExt(p: string): boolean {
  const e = extOf(p);
  return e !== "" && SUPPORTED_EXT.includes(e);
}

function formatDuration(secs: number): string {
  const m = Math.floor(secs / 60);
  const s = Math.round(secs % 60);
  return m > 0 ? `${m} min ${s.toString().padStart(2, "0")} s` : `${s} s`;
}

function fmtTime(secs: number): string {
  const m = Math.floor(secs / 60);
  const s = Math.floor(secs % 60);
  return `${m}:${s.toString().padStart(2, "0")}`;
}
