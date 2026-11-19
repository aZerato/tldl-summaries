import { useEffect, useRef, useState } from "react";
import { confirm } from "@tauri-apps/plugin-dialog";
import {
  deleteContext,
  listContexts,
  saveContext,
  setContextFavorite,
} from "../api/tauri";
import type { PromptContext } from "../types";

function fmtDate(epoch: number): string {
  return new Date(epoch * 1000).toLocaleDateString();
}

/** Page dédiée : création / gestion des contextes de prompt (F15). */
export default function ContextsPage() {
  const [rows, setRows] = useState<PromptContext[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  // Formulaire : `editId` null = création, sinon modification.
  const [editId, setEditId] = useState<number | null>(null);
  const [name, setName] = useState("");
  const [instructions, setInstructions] = useState("");
  const [saving, setSaving] = useState(false);
  // F15 : ancre du formulaire (l'édition y remonte — voir `edit`).
  const formRef = useRef<HTMLDivElement>(null);

  async function load() {
    const r = await listContexts();
    setRows(r);
    setError(null);
    return r;
  }

  useEffect(() => {
    load().catch((e) => {
      setError(String(e));
      setRows([]);
    });
  }, []);

  function reset() {
    setEditId(null);
    setName("");
    setInstructions("");
  }

  function edit(c: PromptContext) {
    setEditId(c.id);
    setName(c.name);
    setInstructions(c.instructions);
    // F15 : l'édition se fait dans le formulaire ci-dessus — on y remonte
    // pour que la modification soit immédiatement visible.
    formRef.current?.scrollIntoView({ behavior: "smooth", block: "start" });
  }

  async function submit() {
    if (!name.trim() || saving) return;
    setSaving(true);
    try {
      await saveContext(editId, name.trim(), instructions.trim());
      reset();
      await load();
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }

  async function remove(c: PromptContext) {
    // Confirmation native (plugin dialog) : gate *avant* la suppression —
    // `window.confirm` n'est pas fiable sous le webview Tauri 2.
    const ok = await confirm(
      `Supprimer le contexte « ${c.name} » ?`,
      {
        title: "Supprimer le contexte",
        kind: "warning",
        okLabel: "Supprimer",
        cancelLabel: "Annuler",
      },
    );
    if (!ok) return;
    try {
      await deleteContext(c.id);
      if (editId === c.id) reset();
      await load();
    } catch (e) {
      setError(String(e));
    }
  }

  async function fav(c: PromptContext) {
    try {
      await setContextFavorite(c.id, !c.isFavorite);
      await load();
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <div className="page">
      <h2>Contextes de prompt</h2>
      <p className="muted">
        Chaque contexte oriente le <strong>résumé</strong> uniquement (F15) :
        il est injecté dans le prompt LLM. La transcription (ASR) n'est
        jamais modifiée — elle reste fidèle à l'audio. À l'import,
        choisissez-le avant de lancer la synthèse — les favoris (★) restent
        épinglés en tête de liste.
      </p>

      {error && (
        <div className="error" role="alert">
          {error}
        </div>
      )}

      <div className="context-form" ref={formRef}>
        <h3>{editId === null ? "Nouveau contexte" : "Modifier le contexte"}</h3>
        <label className="field">
          Nom
          <input
            type="text"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="ex. Rapport de contrôle qualité"
          />
        </label>
        <label className="field">
          Instructions
          <textarea
            rows={5}
            value={instructions}
            onChange={(e) => setInstructions(e.target.value)}
            placeholder="ex. Produis un rapport : constats verbatim, non-conformités, actions correctives. N'invente aucune mesure ; « rien à signaler » est accepté."
          />
        </label>
        <div className="row">
          <button
            className="btn primary"
            disabled={!name.trim() || saving}
            onClick={submit}
          >
            {editId === null
              ? "Créer le contexte"
              : "Enregistrer les modifications"}
          </button>
          {editId !== null && (
            <button className="btn" onClick={reset}>
              Annuler
            </button>
          )}
        </div>
      </div>

      {rows === null ? (
        <p className="muted">Chargement…</p>
      ) : rows.length === 0 ? (
        <p className="muted">Aucun contexte pour l'instant.</p>
      ) : (
        <div className="history-list">
          {rows.map((c) => (
            <div key={c.id} className="history-row">
              <div className="row" style={{ marginTop: 0 }}>
                <button
                  className="btn fav"
                  title={
                    c.isFavorite ? "Retirer des favoris" : "Ajouter aux favoris"
                  }
                  onClick={() => fav(c)}
                >
                  {c.isFavorite ? "★" : "☆"}
                </button>
                <span className="history-file">{c.name}</span>
                <span className="muted">{fmtDate(c.created)}</span>
                <div
                  className="row"
                  style={{ marginLeft: "auto", marginTop: 0 }}
                >
                  <button className="btn" onClick={() => edit(c)}>
                    Modifier
                  </button>
                  <button className="btn danger" onClick={() => remove(c)}>
                    Supprimer
                  </button>
                </div>
              </div>
              <span className="history-excerpt">{c.instructions}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
