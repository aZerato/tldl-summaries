import { useEffect, useState } from "react";
import { getHistory, listHistory, searchHistory } from "../api/tauri";
import type {
  HistoryRecord,
  PipelineOutput,
  Transcription,
} from "../types";

interface Props {
  onOpen: (result: PipelineOutput) => void;
}

function fmtDate(epoch: number): string {
  return new Date(epoch * 1000).toLocaleString();
}

function fmtDur(secs: number): string {
  const m = Math.floor(secs / 60);
  const s = Math.round(secs % 60);
  return m > 0 ? `${m} min ${s} s` : `${s} s`;
}

function baseName(p: string): string {
  return p.split(/[\\/]/).filter(Boolean).pop() ?? p;
}

/** Reconstruit le résultat affiché à partir d'un enregistrement (F7). */
function toOutput(r: HistoryRecord): PipelineOutput {
  let transcription: Transcription;
  if (r.transcriptionJson) {
    try {
      transcription = JSON.parse(r.transcriptionJson) as Transcription;
    } catch {
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
    durationSecs: r.durationSecs,
  };
}

export default function HistoryPage({ onOpen }: Props) {
  const [rows, setRows] = useState<HistoryRecord[] | null>(null);
  const [query, setQuery] = useState("");
  const [opening, setOpening] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Liste + recherche plein texte (F7) : debounce 300 ms.
  useEffect(() => {
    const q = query.trim();
    if (q.length === 0) {
      listHistory()
        .then(setRows)
        .catch((e) => {
          setError(String(e));
          setRows([]);
        });
      return;
    }
    const t = setTimeout(() => {
      searchHistory(q)
        .then(setRows)
        .catch((e) => {
          setError(String(e));
          setRows([]);
        });
    }, 300);
    return () => clearTimeout(t);
  }, [query]);

  function open(r: HistoryRecord) {
    if (opening !== null) return;
    setOpening(r.id);
    getHistory(r.id)
      .then((rec) => onOpen(toOutput(rec)))
      .catch((e) => setError(String(e)))
      .finally(() => setOpening(null));
  }

  return (
    <div className="page">
      <h2>Historique</h2>
      <p className="muted">
        Traitements conservés localement (SQLite). Clic sur une ligne pour
        rouvrir le résultat (F7).
      </p>

      <label className="field">
        Recherche
        <input
          className="search"
          type="search"
          placeholder="fichier, transcription, résumé…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
      </label>

      {error && (
        <div className="error" role="alert">
          {error}
        </div>
      )}

      {rows === null ? (
        <p className="muted">Chargement…</p>
      ) : rows.length === 0 ? (
        <p className="muted">
          {query.trim().length > 0
            ? "Aucun résultat."
            : "Aucun traitement pour l'instant."}
        </p>
      ) : (
        <div className="history-list">
          {rows.map((r) => (
            <div
              key={r.id}
              className={
                opening === r.id ? "history-row opening" : "history-row"
              }
              role="button"
              tabIndex={0}
              onClick={() => open(r)}
              onKeyDown={(e) => {
                if (e.key === "Enter") open(r);
              }}
            >
              <span className="history-file">{baseName(r.file)}</span>
              <span className="muted">
                {fmtDate(r.created)} · {fmtDur(r.durationSecs)} · ASR{" "}
                {r.asrModel} · LLM {r.llmModel}
              </span>
              <span className="history-excerpt">
                {r.transcription.length > 140
                  ? r.transcription.slice(0, 140) + "…"
                  : r.transcription}
              </span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
