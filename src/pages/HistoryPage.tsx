import { useEffect, useRef, useState } from "react";
import { confirm } from "@tauri-apps/plugin-dialog";
import {
  createFolder,
  deleteFolder,
  deleteHistory,
  getHistory,
  listFolders,
  listHistoryFolder,
  moveFileToFolder,
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

/** F19 : glissée en cours de geste (avant/après le seuil de mouvement).
 *  `kind` : une ligne (un résumé) ou un GROUPE (tous les résumés d'un même
 *  fichier). `started` = le seuil de mouvement est franchi (vraie glissée). */
type Grab =
  | { kind: "record"; rec: HistoryRecord; x0: number; y0: number; started: boolean }
  | { kind: "file"; file: string; x0: number; y0: number; started: boolean };

/** `null` = « Sans dossier » (l'état par défaut du rangement — F19). */
export default function HistoryPage({ onOpen }: Props) {
  const [rows, setRows] = useState<HistoryRecord[] | null>(null);
  const [folders, setFolders] = useState<Folder[] | null>(null);
  // F19 : le dossier « ouvert » (`null` = « Sans dossier »).
  const [view, setView] = useState<number | null>(null);
  const [query, setQuery] = useState("");
  const [opening, setOpening] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  // F19 : drag & drop par POINTUEUR (pression + déplacement + relâche) —
  // découplé du dépôt de fichiers OS (F1, `dragDropEnabled` natif de wry) :
  // sous WebView2/Windows, les événements de drag HTML5 sont interceptés par
  // la cible native (IDropTarget/RegisterDragDrop) avant d'atteindre la page
  // (dragover/drop ne partent plus) ; les événements de POINTUEUR, eux, non.
  // Le glisser-déposer F19 (ligne / groupe → dossier) est donc piloté par
  // pointeur, sans HTML5 DnD, et le F1 (dépôt de fichiers OS) reste natif.
  //   - `drag` : la glissée active (rendue = ghost + surbrillance de cible)
  //   - `dropOver` : cible sous le pointeur (id dossier, `"none"`, `null`)
  const [drag, setDrag] = useState<Grab | null>(null);
  const [dropOver, setDropOver] = useState<number | "none" | null>(null);
  // Geste en cours (partagé par les handlers de pointeur de `window`) — ref.
  const grabRef = useRef<Grab | null>(null);
  // Dernier point (positionner le ghost au 1er rendu, sans re-rendu/move).
  const lastPointRef = useRef({ x: 0, y: 0 });
  const ghostRef = useRef<HTMLDivElement | null>(null);
  // Actions fraîches (doMove/doMoveGroup/open) pour les handlers de `window`
  // (montés une fois) — évite les fermetures obsolètes sur `query`/`view`.
  const actionsRef = useRef({ doMove, doMoveGroup, open });
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

  // Actions fraîches pour les handlers de pointeur de `window` (ci-dessous) :
  // `doMove`/`doMoveGroup`/`open` sont recréés à chaque rendu (closures sur
  // `query`/`view`/`opening`) — on les pointe toujours vers les dernières.
  useEffect(() => {
    actionsRef.current = { doMove, doMoveGroup, open };
  });

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

  // F19 : déplacement d'un GROUPE — tous les résumés d'un même fichier
  // (même transcription) d'un seul coup (atomique côté Rust).
  async function doMoveGroup(file: string, fid: number | null) {
    try {
      await moveFileToFolder(file, fid);
      refresh();
    } catch (e) {
      setError(String(e));
    }
  }

  // ── F19 : drag & drop par POINTUEUR (pression + déplacement + relâche) ──
  //
  // Découplé du dépôt de fichiers OS (F1, `dragDropEnabled: true` natif de
  // wry) : sous WebView2/Windows, la cible native (IDropTarget /
  // RegisterDragDrop) intercepte les événements de drag HTML5 (dragover /
  // drop) avant la page — d'où un drag HTML5 qui « ne part pas » à la page.
  // Les événements de POINTUEUR (pointerdown/pointermove/pointerup), eux, ne
  // le sont pas : on pilote donc le glisser-déposer F19 par pointeur, sans
  // HTML5 DnD, et le F1 (dépôt de fichiers OS) reste natif.
  //
  // Cible de drop : l'élément `[data-folder-id]` sous le pointeur
  // (« Sans dossier » = `data-folder-id="none"`). `elementFromPoint` est
  // indifférent au ghost (il a `pointer-events: none`).
  function hitTest(x: number, y: number): number | "none" | null {
    const el = document.elementFromPoint(x, y);
    if (!el) return null;
    const f = el.closest('[data-folder-id]');
    if (!f) return null;
    const v = f.getAttribute("data-folder-id");
    return v === "none" ? "none" : Number(v);
  }

  // Prise (pointerdown) — ligne (un résumé). Pas de prise si on démarre sur
  // un contrôle (select/bouton) : il ne doit pas emporter la ligne.
  function onRowPointerDown(e: React.PointerEvent, r: HistoryRecord) {
    if (e.pointerType === "mouse" && e.button !== 0) return;
    const el = document.elementFromPoint(e.clientX, e.clientY);
    if (el && el.closest("select, button, input")) return;
    grabRef.current = {
      kind: "record",
      rec: r,
      x0: e.clientX,
      y0: e.clientY,
      started: false,
    };
    // Capture : garantit un `pointerup` même si relâché hors de la fenêtre.
    e.currentTarget.setPointerCapture(e.pointerId);
  }

  // Prise (pointerdown) — en-tête de GROUPE (tous les résumés du fichier).
  function onGroupHeadPointerDown(e: React.PointerEvent, file: string) {
    if (e.pointerType === "mouse" && e.button !== 0) return;
    const el = document.elementFromPoint(e.clientX, e.clientY);
    if (el && el.closest("select, button, input")) return;
    grabRef.current = {
      kind: "file",
      file,
      x0: e.clientX,
      y0: e.clientY,
      started: false,
    };
    e.currentTarget.setPointerCapture(e.pointerId);
  }

  // Handlers de `window` (montés une fois) : le geste vit ici, pas dans le
  // JSX. `pointermove` = seuil + hit-test + ghost ; `pointerup` = commit.
  useEffect(() => {
    const onMove = (e: PointerEvent) => {
      const g = grabRef.current;
      if (!g) return;
      lastPointRef.current = { x: e.clientX, y: e.clientY };
      if (!g.started) {
        // Pas encore de glissée tant qu'on n'a pas franchi le seuil — un
        // simple clic (pas de mouvement) ouvre la ligne, sans drag.
        if (Math.hypot(e.clientX - g.x0, e.clientY - g.y0) < 5) return;
        g.started = true;
        setDrag({ ...g });
        document.body.classList.add("tldl-dnd");
      }
      e.preventDefault();
      setDropOver(hitTest(e.clientX, e.clientY));
      const gh = ghostRef.current;
      if (gh) {
        gh.style.left = e.clientX + "px";
        gh.style.top = e.clientY + "px";
      }
    };
    const onUp = (e: PointerEvent) => {
      const g = grabRef.current;
      if (!g) return;
      grabRef.current = null;
      document.body.classList.remove("tldl-dnd");
      if (g.started) {
        const target = hitTest(e.clientX, e.clientY);
        setDrag(null);
        setDropOver(null);
        if (target != null) {
          if (g.kind === "record") {
            actionsRef.current.doMove(
              g.rec.id,
              target === "none" ? null : target,
            );
          } else {
            actionsRef.current.doMoveGroup(
              g.file,
              target === "none" ? null : target,
            );
          }
        }
      } else {
        // Pas de glissée : c'est un CLIC — ouvre la ligne (pas de commit).
        if (g.kind === "record") actionsRef.current.open(g.rec);
        setDrag(null);
        setDropOver(null);
      }
    };
    // Annulation système : on nettoie sans commit.
    const onCancel = () => {
      grabRef.current = null;
      document.body.classList.remove("tldl-dnd");
      setDrag(null);
      setDropOver(null);
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("pointercancel", onCancel);
    return () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", onCancel);
    };
  }, []);

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

  // Libellé du ghost, selon la glissée (une ligne ou un groupe entier).
  let ghostLabel = "";
  if (drag !== null) {
    if (drag.kind === "record") {
      ghostLabel = drag.rec.contextName
        ? `« ${drag.rec.contextName} »`
        : "Contexte par défaut";
    } else {
      const g = groups.find((x) => x.file === drag.file);
      const n = g ? g.rows.length : 1;
      ghostLabel = `Groupe · ${baseName(drag.file)} (${n} ${n === 1 ? "résumé" : "résumés"})`;
    }
  }

  return (
    <div className="page wide">
      <h2>Historique</h2>
      <p className="muted">
        Traitements conservés localement (SQLite), groupés par fichier :
        même transcription, une ligne par variante de résumé (contexte +
        date — F15/F16). Glissez une ligne (ou un en-tête de groupe) sur un
        dossier pour le ranger (F19) ; « Sans dossier » le sort.
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
            data-folder-id="none"
            role="button"
            tabIndex={0}
            onClick={() => setView(null)}
            onKeyDown={(e) => {
              if (e.key === "Enter") setView(null);
            }}
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
              data-folder-id={String(f.id)}
              role="button"
              tabIndex={0}
              onClick={() => setView(f.id)}
              onKeyDown={(e) => {
                if (e.key === "Enter") setView(f.id);
              }}
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
          <p className="muted" style={{ fontSize: 11, opacity: 0.85 }}>
            Glissez une ligne (un résumé) ou un en-tête de groupe (tous les
            résumés d'un fichier) sur un dossier pour le ranger ;
            « Sans dossier » pour le sortir.
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
                  <div
                    className={
                      "history-group-head" +
                      (drag?.kind === "file" && drag.file === g.file
                        ? " dragging"
                        : "")
                    }
                    onPointerDown={(e) => onGroupHeadPointerDown(e, g.file)}
                  >
                    <span className="history-file" title={g.file}>
                      {g.baseName}
                    </span>
                    <select
                      className="folder-pick"
                      title="Ranger tous les résumés de ce fichier (F19)"
                      value={
                        g.rows.every(
                          (r) => r.folderId === g.rows[0].folderId,
                        )
                          ? g.rows[0].folderId === null
                            ? ""
                            : String(g.rows[0].folderId)
                          : "mixed"
                      }
                      onClick={(e) => e.stopPropagation()}
                      onChange={(e) => {
                        const v = e.target.value;
                        if (v === "mixed") return;
                        doMoveGroup(g.file, v === "" ? null : Number(v));
                      }}
                    >
                      <option value="">Sans dossier</option>
                      <option value="mixed" disabled>
                        Groupe partagé…
                      </option>
                      {folders?.map((f) => (
                        <option key={f.id} value={f.id}>
                          {f.name}
                        </option>
                      ))}
                    </select>
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
                        "history-row" +
                        (opening === r.id ? " opening" : "") +
                        (drag?.kind === "record" && drag.rec.id === r.id
                          ? " dragging"
                          : "")
                      }
                      role="button"
                      tabIndex={0}
                      onPointerDown={(e) => onRowPointerDown(e, r)}
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

      {/* Ghost de glisser-déposer (F19, piloté par pointeur) : suit le
       *  curseur, `pointer-events: none` (ne gêne pas le hit-test). */}
      {drag !== null && (
        <div
          ref={ghostRef}
          className="dnd-ghost"
          style={{
            left: lastPointRef.current.x + "px",
            top: lastPointRef.current.y + "px",
          }}
        >
          {ghostLabel}
        </div>
      )}
    </div>
  );
}
