import { useEffect, useRef, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import ConfirmDialog from "../components/ConfirmDialog";
import Progress from "../components/Progress";
import SegmentList from "../components/SegmentList";
import {
  convertFileSrc,
  deleteHistory,
  getHistory,
  historyForFile,
  listContexts,
  listFolders,
  moveRecord,
  reSummarize,
  toOutput,
} from "../api/tauri";
import type {
  Folder,
  HistoryRecord,
  PipelineOutput,
  PromptContext,
  Progress as ProgressEvent,
  ResultMeta,
} from "../types";

interface Props {
  result: PipelineOutput;
  /** F15/F16 : id de l'enregistrement ouvert — `null` = pas de re-résumé
   *  possible depuis cette vue. */
  recordId: number | null;
  /** F15/F16 : contexte + date de l'enregistrement affiché. */
  resultMeta: ResultMeta | null;
  /** F15/F16 : le résultat affiché est remplacé par l'enregistrement donné
   *  (re-résumé terminé ou choix d'un résumé existant) ; `rec` `null` =
   *  résultat en mémoire sans enregistrement consigné (re-résumé
   *  impossible). */
  onShown: (updated: PipelineOutput, rec: HistoryRecord | null) => void;
}

function fmtDate(epoch: number): string {
  return new Date(epoch * 1000).toLocaleString();
}

function fmtTime(secs: number): string {
  const m = Math.floor(secs / 60);
  const s = Math.floor(secs % 60);
  return `${m}:${s.toString().padStart(2, "0")}`;
}

export default function ResultPage({
  result,
  recordId,
  resultMeta,
  onShown,
}: Props) {
  // Pas de résumé ? On ouvre directement la transcription.
  const [tab, setTab] = useState<"resume" | "transcription">(
    result.summaryMarkdown ? "resume" : "transcription"
  );
  const [copied, setCopied] = useState(false);

  // F13 : lecteur du média source (protocole asset Tauri), synchronisé
  // aux segments — clic sur un segment = saut + reprise à ce segment.
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const [currentTime, setCurrentTime] = useState(0);
  const [playing, setPlaying] = useState(false);
  const [mediaMissing, setMediaMissing] = useState(false);

  // F15/F16 : re-résumé avec un autre contexte depuis le résultat — LLM
  // seul, la transcription stockée est réutilisée (jamais re-transcrite).
  const [contexts, setContexts] = useState<PromptContext[]>([]);
  const [contextId, setContextId] = useState<number | null>(null);
  const [resuming, setResuming] = useState(false);
  const [resumError, setResumError] = useState<string | null>(null);
  // F15/F16 : barre d'avancement du re-résumé — `re_summarize` émet
  // `pipeline-progress` (étapes `llm` → `done`), même contrat que le
  // pipeline complet de l'import.
  const [progress, setProgress] = useState<ProgressEvent | null>(null);

  // F15/F16 : résumés par contexte — tous les résumés du fichier sont
  // conservés (chaque re-résumé en consigne un nouveau, contexte + date
  // tracés) ; clic sur une ligne = ouvrir ce résumé sans recalcul.
  const [summaries, setSummaries] = useState<HistoryRecord[] | null>(null);
  const [opening, setOpening] = useState<number | null>(null);

  // F19 : dossiers de rangement — le sélecteur ici est l'alternative au
  // glisser-déposer de la page Historique.
  const [folders, setFolders] = useState<Folder[]>([]);
  // Suppression d'une variante de résumé : `toDelete` posé = boîte
  // ouverte (pas de `confirm` natif — pas fiable sous le webview Tauri 2).
  const [toDelete, setToDelete] = useState<HistoryRecord | null>(null);

  useEffect(() => {
    let cancelled = false;
    listContexts()
      .then((c) => {
        if (cancelled) return;
        setContexts(c);
        // Un contexte disparu (supprimé) ne doit pas rester sélectionné.
        setContextId((id) =>
          id === null || c.some((x) => x.id === id) ? id : null,
        );
      })
      .catch((e) => {
        if (!cancelled) setResumError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // La liste se rafraîchit quand l'enregistrement affiché change
  // (re-résumé, ouverture d'un autre résumé du même fichier).
  useEffect(() => {
    let cancelled = false;
    historyForFile(result.file)
      .then((rows) => {
        if (!cancelled) setSummaries(rows);
      })
      .catch((e) => {
        if (!cancelled) setResumError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [result.file, recordId]);

  // F19 : catalogue des dossiers (une seule fois).
  useEffect(() => {
    let cancelled = false;
    listFolders()
      .then((fs) => {
        if (!cancelled) setFolders(fs);
      })
      .catch(() => {
        if (!cancelled) setFolders([]);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // F15/F16 : la commande `re_summarize` émet `pipeline-progress`
  // (étapes `llm` → `done`) — même contrat que le pipeline complet de
  // l'import ; on l'affiche ici sous la barre de re-résumé.
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

  const segments = result.transcription.segments;
  const activeIndex = playing
    ? segments.findIndex((s) => s.start <= currentTime && currentTime < s.end)
    : -1;

  function seekTo(t: number) {
    const a = audioRef.current;
    if (!a) return;
    a.currentTime = t;
    void a.play().catch(() => undefined);
  }

  async function copy() {
    const text =
      tab === "resume"
        ? result.summaryMarkdown
        : result.transcription.text;
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      // presse-papiers indisponible
    }
  }

  // F15/F16 : re-résumé — seule l'étape LLM est relancée (contexte choisi),
  // sur la transcription stockée ; un nouvel enregistrement est consigné
  // (l'ancien reste intact dans l'historique).
  async function resume() {
    if (recordId === null || resuming) return;
    setResuming(true);
    setResumError(null);
    setProgress(null);
    try {
      const rec = await reSummarize(recordId, contextId);
      onShown(toOutput(rec), rec);
    } catch (e) {
      setResumError(String(e));
    } finally {
      setResuming(false);
      setProgress(null);
    }
  }

  // Ouverture d'un résumé existant du même fichier — pas de recalcul.
  async function openSummary(rec: HistoryRecord) {
    if (rec.id === recordId || opening !== null) return;
    setOpening(rec.id);
    getHistory(rec.id)
      .then((r) => onShown(toOutput(r), r))
      .catch((e) => setResumError(String(e)))
      .finally(() => setOpening(null));
  }

  // F15/F16 : suppression d'une variante de résumé de ce fichier
  // (confirmation in-app, comme l'historique F7). Si l'enregistrement
  // supprimé est celui affiché : on ouvre le plus récent restant — ou
  // l'on garde le résultat en mémoire s'il n'en reste aucun (re-résumé
  // désactivé : plus d'enregistrement sous-jacent).
  function removeSummary(rec: HistoryRecord) {
    if (resuming || opening !== null) return;
    setToDelete(rec);
  }

  async function doDeleteSummary() {
    const rec = toDelete;
    if (!rec) return;
    try {
      await deleteHistory(rec.id);
      const rows = await historyForFile(result.file);
      setSummaries(rows);
      if (rec.id === recordId) {
        if (rows.length > 0) {
          // `history_for_file` : plus récents d'abord (id DESC).
          onShown(toOutput(rows[0]), rows[0]);
        } else {
          onShown(result, null);
        }
      }
    } catch (e) {
      setResumError(String(e));
    }
  }

  // F19 : rangement de ce résultat dans un dossier (ou « Sans dossier »).
  async function fileRecord(fid: number | null) {
    if (recordId === null) return;
    try {
      await moveRecord(recordId, fid);
      const rows = await historyForFile(result.file);
      setSummaries(rows);
    } catch (e) {
      setResumError(String(e));
    }
  }

  // F19 : rangement courant de l'enregistrement affiché.
  const currentRec = summaries?.find((r) => r.id === recordId);

  return (
    <div className="page">
      <div className="row spread">
        <div className="tabs">
          <button
            className={tab === "resume" ? "tab active" : "tab"}
            onClick={() => setTab("resume")}
          >
            Résumé
          </button>
          <button
            className={tab === "transcription" ? "tab active" : "tab"}
            onClick={() => setTab("transcription")}
          >
            Transcription
          </button>
        </div>
        <button className="btn" onClick={copy}>
          {copied ? "Copié ✓" : "Copier"}
        </button>
      </div>

      {/* F13 : média source. Le fichier intermédiaire WAV a été effacé en
          fin de pipeline — on lit le fichier d'origine, pas la conversion. */}
      {mediaMissing ? (
        <p className="muted player-note">
          Média source indisponible (fichier déplacé ou supprimé ?) —
          lecture impossible.
        </p>
      ) : (
        <audio
          ref={audioRef}
          className="player"
          controls
          src={convertFileSrc(result.file)}
          onTimeUpdate={(e) => setCurrentTime(e.currentTarget.currentTime)}
          onPlay={() => setPlaying(true)}
          onPause={() => setPlaying(false)}
          onEnded={(e) =>
            setCurrentTime(e.currentTarget.duration || e.currentTarget.currentTime)
          }
          onError={() => setMediaMissing(true)}
        />
      )}

      {/* F15/F16 : re-résumé avec un autre contexte — LLM seul, la
          transcription stockée est réutilisée, pas de re-transcription. */}
      {recordId !== null && (
        <div className="resum">
          <span className="muted">
            Relancer le résumé — LLM seul, transcription conservée
            {resultMeta
              ? ` · contexte actuel : ${
                  resultMeta.contextName || "par défaut"
                }`
              : ""}
          </span>
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
          <button
            className="btn primary"
            disabled={resuming}
            onClick={resume}
          >
            {resuming ? "Résumé en cours…" : "Relancer le résumé"}
          </button>
        </div>
      )}
      {resumError && <div className="error">{resumError}</div>}
      {progress && <Progress progress={progress} />}

      {/* F19 : rangement du résultat — alternative au glisser-déposer de
          la page Historique. */}
      {recordId !== null && summaries !== null && (
        <div className="row">
          <span className="muted">Rangement</span>
          <select
            className="folder-pick"
            value={
              currentRec
                ? currentRec.folderId === null
                  ? ""
                  : String(currentRec.folderId)
                : ""
            }
            onChange={(e) => {
              const v = e.target.value;
              void fileRecord(v === "" ? null : Number(v));
            }}
          >
            <option value="">Sans dossier</option>
            {folders.map((f) => (
              <option key={f.id} value={f.id}>
                {f.name}
              </option>
            ))}
          </select>
        </div>
      )}

      {tab === "resume" ? (
        result.summaryMarkdown ? (
          <pre className="markdown">{result.summaryMarkdown}</pre>
        ) : (
          <p className="muted">
            {result.summaryError ?? "Résumé indisponible."}
          </p>
        )
      ) : (
        <>
          {result.diarError && (
            <p className="muted" role="alert">
              {result.diarError}
            </p>
          )}
          <SegmentList
            segments={segments}
            onSeek={seekTo}
            activeIndex={activeIndex}
          />
        </>
      )}

      {/* F15/F16 : résumés par contexte — chaque re-résumé d'un même
          fichier est conservé (contexte + horodatage tracés). */}
      {summaries !== null && summaries.length > 0 && (
        <>
          <h3 className="summ-title">
            Résumés de ce fichier ({summaries.length})
          </h3>
          <div className="history-list">
            {summaries.map((r) => (
              <div
                key={r.id}
                className={
                  [
                    "history-row",
                    opening === r.id ? "opening" : "",
                    r.id === recordId ? "current" : "",
                  ]
                    .filter(Boolean)
                    .join(" ")
                }
                role="button"
                tabIndex={0}
                onClick={() => openSummary(r)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") openSummary(r);
                }}
              >
                <div className="row" style={{ marginTop: 0 }}>
                  <span className="history-file">
                    {r.contextName
                      ? `« ${r.contextName} »`
                      : "Contexte par défaut"}
                  </span>
                  {r.id === recordId && <span className="badge">affiché</span>}
                  <span
                    className="muted"
                    style={{ marginLeft: "auto" }}
                  >
                    {fmtDate(r.created)} · LLM {r.llmModel}
                    {r.device ? ` · ${r.device}` : ""}
                  </span>
                  <button
                    className="btn danger"
                    disabled={resuming}
                    onClick={(e) => {
                      e.stopPropagation();
                      removeSummary(r);
                    }}
                  >
                    Supprimer
                  </button>
                </div>
                <span className="history-excerpt">
                  {r.summaryMarkdown.length > 140
                    ? r.summaryMarkdown.slice(0, 140) + "…"
                    : r.summaryMarkdown}
                </span>
              </div>
            ))}
          </div>
        </>
      )}

      {result.deviceNote && (
        <div className="error" role="alert">
          {result.deviceNote}
        </div>
      )}

      <div className="muted">
        {resultMeta && (
          <>
            Résumé :{" "}
            {resultMeta.contextName ? `« ${resultMeta.contextName} »` : "contexte par défaut"}
            {" · "}{fmtDate(resultMeta.created)} ·{" "}
          </>
        )}
        ASR : {result.asrModel} · LLM : {result.llmModel} · Device :{" "}
        {result.device} · {Math.round(result.durationSecs)} s de média ·{" "}
        {result.sectionStart > 0 || result.sectionEnd < result.durationSecs ? (
          <span title="Section traitée — les segments sont décalés du début de section pour rester alignés sur le fichier source">
            section {fmtTime(result.sectionStart)} → {fmtTime(result.sectionEnd)}
          </span>
        ) : (
          "fichier complet"
        )}
      </div>

      {toDelete && (
        <ConfirmDialog
          title="Supprimer le résumé"
          message={`Supprimer le résumé « ${
            toDelete.contextName || "contexte par défaut"
          } » ?`}
          onConfirm={doDeleteSummary}
          onCancel={() => setToDelete(null)}
        />
      )}
    </div>
  );
}
