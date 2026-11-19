import { useEffect, useState } from "react";
import { confirm } from "@tauri-apps/plugin-dialog";
import {
  deleteHistory,
  getHistory,
  listHistory,
  searchHistory,
  toOutput,
} from "../api/tauri";
import type { HistoryRecord, PipelineOutput } from "../types";

interface Props {
  /** F15/F16 : le résultat + l'enregistrement ouvert (re-résumé, résumés
   *  par contexte). */
  onOpen: (result: PipelineOutput, rec: HistoryRecord) => void;
}

/** F7/F15 : un groupe par fichier — la transcription est commune, les
 *  lignes sont les variantes de résumé (contexte + date) ; clic = ouvrir. */
interface FileGroup {
  file: string;
  baseName: string;
  /** Enregistrements, plus récents en premier. */
  rows: HistoryRecord[];
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

// Les enregistrements arrivent plus récents d'abord (`list_history` /
// `search_history` triés) ; les groupes sont ordonnés de même (le plus
// récent d'abord).
function groupByFile(rows: HistoryRecord[]): FileGroup[] {
  const byFile = new Map<string, HistoryRecord[]>();
  for (const r of rows) {
    const arr = byFile.get(r.file);
    if (arr) {
      arr.push(r);
    } else {
      byFile.set(r.file, [r]);
    }
  }
  return [...byFile.values()]
    .map((rs) => ({
      file: rs[0].file,
      baseName: baseName(rs[0].file),
      rows: rs,
    }))
    .sort((a, b) => b.rows[0].created - a.rows[0].created);
}

export default function HistoryPage({ onOpen }: Props) {
  const [rows, setRows] = useState<HistoryRecord[] | null>(null);
  const [query, setQuery] = useState("");
  const [opening, setOpening] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Recharge la liste (sans debounce — ex. après une suppression).
  function refresh() {
    const q = query.trim();
    if (q.length === 0) {
      listHistory()
        .then(setRows)
        .catch((e) => {
          setError(String(e));
          setRows([]);
        });
    } else {
      searchHistory(q)
        .then(setRows)
        .catch((e) => {
          setError(String(e));
          setRows([]);
        });
    }
  }

  // Liste + recherche plein texte (F7) : debounce 300 ms.
  useEffect(() => {
    const q = query.trim();
    if (q.length === 0) {
      refresh();
      return;
    }
    const t = setTimeout(() => refresh(), 300);
    return () => clearTimeout(t);
  }, [query]);

  function open(r: HistoryRecord) {
    if (opening !== null) return;
    setOpening(r.id);
    getHistory(r.id)
      .then((rec) => onOpen(toOutput(rec), rec))
      .catch((e) => setError(String(e)))
      .finally(() => setOpening(null));
  }

  async function remove(r: HistoryRecord) {
    // Confirmation native (plugin dialog) — gate *avant* la suppression.
    const ok = await confirm(
      `Supprimer « ${baseName(r.file)} » de l'historique ?`,
      {
        title: "Supprimer de l'historique",
        kind: "warning",
        okLabel: "Supprimer",
        cancelLabel: "Annuler",
      },
    );
    if (!ok) return;
    try {
      await deleteHistory(r.id);
      refresh();
    } catch (e) {
      setError(String(e));
    }
  }

  // `rows === null` = chargement ; sinon `groups` est toujours un tableau
  // (vide si aucun enregistrement).
  const groups = rows === null ? [] : groupByFile(rows);

  return (
    <div className="page">
      <h2>Historique</h2>
      <p className="muted">
        Traitements conservés localement (SQLite), groupés par fichier :
        même transcription, une ligne par variante de résumé (contexte +
        date — F15/F16). Clic sur une ligne pour rouvrir le résultat (F7).
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
      ) : groups.length === 0 ? (
        <p className="muted">
          {query.trim().length > 0
            ? "Aucun résultat."
            : "Aucun traitement pour l'instant."}
        </p>
      ) : (
        <div className="history-list">
          {groups.map((g) => (
            <div className="history-group" key={g.file}>
              <div className="history-group-head">
                <span className="history-file" title={g.file}>
                  {g.baseName}
                </span>
                <span className="muted" style={{ marginLeft: "auto" }}>
                  {g.rows.length}{" "}
                  {g.rows.length > 1 ? "résumés" : "résumé"} ·{" "}
                  {fmtDur(g.rows[0].durationSecs)} · ASR{" "}
                  {g.rows[0].asrModel}
                </span>
              </div>
              {g.rows.map((r) => (
                <div
                  key={r.id}
                  className={
                    opening === r.id
                      ? "history-row opening"
                      : "history-row"
                  }
                  role="button"
                  tabIndex={0}
                  onClick={() => open(r)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") open(r);
                  }}
                >
                  <div className="row" style={{ marginTop: 0 }}>
                    <span className="history-file">
                      {r.contextName
                        ? `« ${r.contextName} »`
                        : "Contexte par défaut"}
                    </span>
                    <span
                      className="muted"
                      style={{ marginLeft: "auto" }}
                    >
                      {fmtDate(r.created)} · LLM {r.llmModel}
                      {r.device ? ` · ${r.device}` : ""}
                    </span>
                    <button
                      className="btn danger"
                      onClick={(e) => {
                        e.stopPropagation();
                        remove(r);
                      }}
                    >
                      Supprimer
                    </button>
                  </div>
                  <span className="history-excerpt">
                    {r.summaryMarkdown.length > 140
                      ? r.summaryMarkdown.slice(0, 140) + "…"
                      : r.summaryMarkdown || "Aucun résumé."}
                  </span>
                </div>
              ))}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
