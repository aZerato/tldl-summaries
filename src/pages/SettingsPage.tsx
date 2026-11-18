import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  cancelModelDownload,
  deleteModel,
  downloadModel,
  listModels,
  setConfig,
} from "../api/tauri";
import type { AppConfig, ModelInfo } from "../types";

interface Props {
  config: AppConfig;
  onChange: (c: AppConfig) => void;
}

interface Dl {
  id: string;
  received: number;
  total: number;
}

const MB = 1024 * 1024;

export default function SettingsPage({ config, onChange }: Props) {
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [saved, setSaved] = useState(false);
  const [dl, setDl] = useState<Dl | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    listModels()
      .then(setModels)
      .catch(() => setModels([]));
    let un: (() => void) | undefined;
    listen("model-download-progress", (e) => {
      const p = e.payload as { id: string; received: number; total: number };
      setDl(
        (d) =>
          d && d.id === p.id ? { id: p.id, received: p.received, total: p.total } : d,
      );
    })
      .then((u) => (un = u))
      .catch(() => undefined);
    return () => {
      un?.();
    };
  }, []);

  const asr = models.filter((m) => m.kind === "asr");
  const llm = models.filter((m) => m.kind === "llm");
  const busy = dl !== null;

  function save(next: AppConfig) {
    onChange(next);
    setSaved(true);
    setConfig(next).catch(() => undefined);
  }

  function startDownload(id: string) {
    setDl({ id, received: 0, total: 0 });
    setError(null);
    downloadModel(id)
      .then(() => {
        setDl(null);
        listModels()
          .then(setModels)
          .catch(() => undefined);
      })
      .catch((err) => {
        setDl(null);
        setError(String(err));
      });
  }

  function doCancel() {
    if (dl) {
      cancelModelDownload(dl.id).catch(() => undefined);
      setDl(null);
    }
  }

  function doDelete(m: ModelInfo) {
    if (!window.confirm(`Supprimer « ${m.name} » de models/ ?`)) return;
    deleteModel(m.id)
      .then(() => listModels().then(setModels).catch(() => undefined))
      .catch((err) => setError(String(err)));
  }

  function dlControl(m: ModelInfo) {
    const active = dl && dl.id === m.id;
    if (active && dl) {
      const total = dl.total > 0 ? dl.total : m.sizeMb * MB;
      const pct = Math.min(100, (dl.received / total) * 100);
      return (
        <span className="dl">
          <span className="dl-bar">
            <span style={{ width: `${pct}%` }} />
          </span>
          <span className="dl-pct">
            {(dl.received / MB).toFixed(0)} / {(total / MB).toFixed(0)} Mo
          </span>
          <button className="btn" onClick={doCancel}>
            Annuler
          </button>
        </span>
      );
    }
    return (
      <button
        className="btn"
        disabled={busy}
        onClick={() => startDownload(m.id)}
      >
        Télécharger ({m.sizeMb} Mo)
      </button>
    );
  }

  function rowCell(m: ModelInfo) {
    if (m.available) {
      return (
        <span className="dl-cell">
          <button className="btn danger" onClick={() => doDelete(m)}>
            Supprimer
          </button>
        </span>
      );
    }
    return <span className="dl-cell">{dlControl(m)}</span>;
  }

  return (
    <div className="page">
      <h2>Modèles</h2>
      <p className="muted">
        Les modèles sont stockés localement, téléchargement opt-in (F11).
        Hors ligne : déposez les fichiers GGUF/.bin à la main dans le dossier
        <code> models/</code> de l'app.
      </p>

      {error && (
        <div className="error" role="alert">
          {error}
        </div>
      )}

      <section className="card">
        <h3>ASR — transcription (whisper.cpp)</h3>
        {asr.map((m) => (
          <div
            key={m.id}
            className={m.id === config.asrModel ? "model-row active" : "model-row"}
          >
            <label className="model-pick">
              <input
                type="radio"
                name="asr"
                checked={m.id === config.asrModel}
                onChange={() => save({ ...config, asrModel: m.id })}
              />
              <span className="model-name">{m.name}</span>
              <span className="muted">
                {m.sizeMb} Mo · ~{m.ramGb} Go RAM
                {m.available ? "" : " · à télécharger"}
              </span>
              {m.isDefault && <span className="badge">défaut</span>}
            </label>
            {rowCell(m)}
          </div>
        ))}
      </section>

      <section className="card">
        <h3>LLM — résumé (llama.cpp)</h3>
        {llm.map((m) => (
          <div
            key={m.id}
            className={m.id === config.llmModel ? "model-row active" : "model-row"}
          >
            <label className="model-pick">
              <input
                type="radio"
                name="llm"
                checked={m.id === config.llmModel}
                onChange={() => save({ ...config, llmModel: m.id })}
              />
              <span className="model-name">{m.name}</span>
              <span className="muted">
                {(m.sizeMb / 1024).toFixed(1).replace(".", ",")} Go ·
                ~{m.ramGb} Go
                {m.available ? "" : " · à télécharger"}
              </span>
              {m.isDefault && <span className="badge">défaut</span>}
            </label>
            {rowCell(m)}
          </div>
        ))}
      </section>

      <section className="card">
        <h3>Général</h3>
        <label className="field">
          Langue
          <select
            value={config.language}
            onChange={(e) => save({ ...config, language: e.target.value })}
          >
            <option value="auto">auto</option>
            <option value="fr">français</option>
            <option value="en">anglais</option>
          </select>
        </label>
        <label className="field">
          Température LLM (
          {config.temperature.toFixed(1).replace(".", ",")})
          <input
            type="range"
            min={0}
            max={1}
            step={0.05}
            value={config.temperature}
            onChange={(e) =>
              save({ ...config, temperature: Number(e.target.value) })
            }
          />
        </label>
        <label className="field">
          Profil (rappel — docs/models.md)
          <select
            value={config.profile}
            onChange={(e) =>
              save({
                ...config,
                profile: e.target.value as AppConfig["profile"],
              })
            }
          >
            <option value="leger">Léger — 8 Go</option>
            <option value="equilibre">Équilibré — 16 Go</option>
            <option value="puissant">Puissant — 32 Go / GPU</option>
          </select>
        </label>
      </section>
      {saved && <p className="muted">Enregistré ✓</p>}
    </div>
  );
}
