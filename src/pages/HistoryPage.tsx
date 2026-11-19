import { useEffect, useState } from "react";
import type { DragEvent } from "react";
import { confirm } from "@tauri-apps/plugin-dialog";
import {
  createFolder,
  deleteFolder,
  deleteHistory,
  getHistory,
  listFolders,
  listHistoryFolder,
  moveRecord,
  searchHistory,
  toOutput,
} from "../api/tauri";
import type { Folder, HistoryRecord, PipelineOutput } from "../types";

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

// Les enregistrements arrivent plus récents d'abord (`list_history_folder`
// / `search_history` triés) ; les groupes sont ordonnés de même (le plus
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

/** `null` = « Sans dossier » (l'état par défaut du rangement — F19). */
export default function HistoryPage({ onOpen }: Props) {
  const [rows, setRows] = useState<HistoryRecord[] | null>(null);
  const [folders, setFolders] = useState<Folder[] | null>(null);
  // F19 : le dossier « ouvert » (`null` = « Sans dossier »).
  const [view, setView] = useState<number | null>(null);
  const [query, setQuery] = useState("");
  const [opening, setOpening] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  // F19 : drag & drop — enregistrement en cours de glissée + cible survolée
  // (`null` = en attente, `"none"` = « Sans dossier »).
  const [draggingId, setDraggingId] = useState<number | null>(null);
  const [dropOver, setDropOver] = useState<number | "none" | null>(null);
  // F19 : création rapide d'un dossier (nom en cours de saisie).
  const [newName, setNewName] = useState("");

  const searching = query.trim().length > 0;

  // Recharge la liste des lignes du dossier ouvert + le catalogue des
  // dossiers (comptes par dossier) — sans debounce ; à appeler après
  // chaque mutation (création/suppression de dossier, déplacement,
  // suppression de ligne).
  function refresh() {
    listFolders()
      .then(setFolders)
      .catch((e) => setError(String(e)));
    const q = query.trim();
    if (q.length === 0) {
      listHistoryFolder(view)
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

  // Chargement au montage + à chaque changement : contenu du dossier
  // ouvert, recherche plein texte (F7 — globale, tous dossiers) et
  // catalogue des dossiers (comptes). Debounce 300 ms.
  useEffect(() => {
    if (!searching) {
      refresh();
      return;
    }
    const t = setTimeout(() => refresh(), 300);
    return () => clearTimeout(t);
  }, [query, view]);

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

  // F19 : déplacement (glisser-déposer ou sélecteur) — vers un dossier, ou
  // « Sans dossier » (`null`).
  async function doMove(id: number, fid: number | null) {
    try {
      await moveRecord(id, fid);
      refresh();
    } catch (e) {
      setError(String(e));
    }
  }

  // F19 : drag & drop HTML5 (dans le webview — indépendant de
  // l'interception native des fichiers OS, cf. F1).
  function onDragStart(e: DragEvent, r: HistoryRecord) {
    e.dataTransfer.setData("text/plain", String(r.id));
    e.dataTransfer.effectAllowed = "move";
    setDraggingId(r.id);
  }

  function onDragEnd() {
    setDraggingId(null);
    setDropOver(null);
  }

  function onFolderDragOver(e: DragEvent, fid: number | "none") {
    if (draggingId === null) return;
    // Sans preventDefault, le drop est refusé par le webview.
    e.preventDefault();
    e.dataTransfer.dropEffect = "move";
    setDropOver((cur) => (cur === fid ? cur : fid));
  }

  function onFolderDrop(e: DragEvent, fid: number | "none") {
    e.preventDefault();
    setDropOver(null);
    const raw = e.dataTransfer.getData("text/plain");
    const id = Number(raw);
    if (!Number.isInteger(id) || id < 1) return;
    doMove(id, fid === "none" ? null : fid);
  }

  // F19 : création rapide (le nouveau dossier s'ouvre).
  async function createFolderNow() {
    const name = newName.trim();
    if (name.length === 0) return;
    try {
      const id = await createFolder(name);
      setNewName("");
      setView(id);
    } catch (e) {
      setError(String(e));
    }
  }

  // F19 : suppression d'un dossier — ses résultats repassent « Sans
  // dossier » (jamais supprimés) ; confirmation native.
  async function removeFolder(f: Folder) {
    const ok = await confirm(
      `Supprimer le dossier « ${f.name} » ? Ses résultats repassent « Sans dossier » (jamais supprimés).`,
      {
        title: "Supprimer le dossier",
        kind: "warning",
        okLabel: "Supprimer",
        cancelLabel: "Annuler",
      },
    );
    if (!ok) return;
    try {
      await deleteFolder(f.id);
      if (view === f.id) setView(null);
      refresh();
    } catch (e) {
      setError(String(e));
    }
  }

  // `rows === null` = chargement ; sinon `groups` est toujours un tableau
  // (vide si aucun enregistrement).
  const groups = rows === null ? [] : groupByFile(rows);
  const viewName =
    view === null
      ? "Sans dossier"
      : folders?.find((f) => f.id === view)?.name ?? "Dossier";

  return (
    <div className="page wide">
      <h2>Historique</h2>
      <p className="muted">
        Traitements conservés localement (SQLite), groupés par fichier :
        même transcription, une ligne par variante de résumé (contexte +
        date — F15/F16). Glissez une ligne sur un dossier pour la ranger
        (F19) ; « Sans dossier » la sort.
      </p>

      <div className="folders-layout">
        <aside className="folders-panel">
          <h3>Dossiers</h3>
          <div className="row" style={{ marginTop: 0 }}>
            <input
              type="text"
              className="search"
              style={{ flex: 1, minWidth: 0 }}
              placeholder="Nouveau dossier…"
              value={newName}
              onChange={(e) => setNewName(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") createFolderNow();
              }}
            />
            <button className="btn primary" onClick={createFolderNow}>
              Créer
            </button>
          </div>
          <div
            className={
              (view === null ? "folder-item active " : "folder-item ") +
              (dropOver === "none" ? "drop-over" : "")
            }
            role="button"
            tabIndex={0}
            onClick={() => setView(null)}
            onKeyDown={(e) => {
              if (e.key === "Enter") setView(null);
            }}
            onDragOver={(e) => onFolderDragOver(e, "none")}
            onDrop={(e) => onFolderDrop(e, "none")}
          >
            <span className="folder-name">Sans dossier</span>
          </div>
          {folders?.map((f) => (
            <div
              key={f.id}
              className={
                (view === f.id ? "folder-item active " : "folder-item ") +
                (dropOver === f.id ? "drop-over" : "")
              }
              role="button"
              tabIndex={0}
              onClick={() => setView(f.id)}
              onKeyDown={(e) => {
                if (e.key === "Enter") setView(f.id);
              }}
              onDragOver={(e) => onFolderDragOver(e, f.id)}
              onDrop={(e) => onFolderDrop(e, f.id)}
            >
              <span className="folder-name" title={f.name}>
                {f.name}
              </span>
              <span className="badge" style={{ marginLeft: "auto" }}>
                {f.count}
              </span>
              <button
                className="btn danger"
                title="Supprimer le dossier"
                onClick={(e) => {
                  e.stopPropagation();
                  removeFolder(f);
                }}
              >
                Suppr.
              </button>
            </div>
          ))}
          <p className="muted">
            Glissez une ligne de l'historique sur un dossier pour la ranger
            ; « Sans dossier » pour la sortir.
          </p>
        </aside>

        <section className="folders-main">
          <div className="row" style={{ marginTop: 0 }}>
            <h3 style={{ margin: 0 }}>{viewName}</h3>
            {view !== null && (
              <button className="btn" onClick={() => setView(null)}>
                Voir « Sans dossier »
              </button>
            )}
          </div>

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
          {searching && (
            <p className="muted">
              Recherche dans tout l'historique (tous dossiers).
            </p>
          )}

          {error && (
            <div className="error" role="alert">
              {error}
            </div>
          )}

          {rows === null ? (
            <p className="muted">Chargement…</p>
          ) : groups.length === 0 ? (
            <p className="muted">
              {searching
                ? "Aucun résultat."
                : view === null
                  ? "Aucun résultat sans dossier."
                  : `Aucun résultat dans « ${viewName} ».`}
            </p>
          ) : (
            <div className="history-list">
              {groups.map((g) => (
                <div className="history-group" key={g.file}>
                  <div className="history-group-head">
                    <span className="history-file" title={g.file}>
                      {g.baseName}
                    </span>
                    <span
                      className="muted"
                      style={{ marginLeft: "auto" }}
                    >
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
                        "history-row draggable " +
                        (opening === r.id ? "opening " : "") +
                        (draggingId === r.id ? "dragging" : "")
                      }
                      role="button"
                      tabIndex={0}
                      draggable
                      onDragStart={(e) => onDragStart(e, r)}
                      onDragEnd={onDragEnd}
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
                        <select
                          className="folder-pick"
                          title="Ranger dans un dossier (F19)"
                          value={
                            r.folderId === null ? "" : String(r.folderId)
                          }
                          onClick={(e) => e.stopPropagation()}
                          onChange={(e) => {
                            const v = e.target.value;
                            doMove(
                              r.id,
                              v === "" ? null : Number(v),
                            );
                          }}
                        >
                          <option value="">Sans dossier</option>
                          {folders?.map((f) => (
                            <option key={f.id} value={f.id}>
                              {f.name}
                            </option>
                          ))}
                        </select>
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
        </section>
      </div>
    </div>
  );
}
