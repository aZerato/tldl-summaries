import { useEffect, useState } from "react";
import { listModels, setConfig } from "../api/tauri";
import type { AppConfig, ModelInfo } from "../types";

interface Props {
  config: AppConfig;
  onChange: (c: AppConfig) => void;
}

export default function SettingsPage({ config, onChange }: Props) {
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    listModels()
      .then(setModels)
      .catch(() => setModels([]));
  }, []);

  const asr = models.filter((m) => m.kind === "asr");
  const llm = models.filter((m) => m.kind === "llm");

  function save(next: AppConfig) {
    onChange(next);
    setSaved(true);
    setConfig(next).catch(() => undefined);
  }

  return (
    <div className="page">
      <h2>Modèles</h2>
      <p className="muted">
        Les modèles sont stockés localement. Téléchargement : à venir (F11) —
        en attendant, déposez les fichiers GGUF/.bin à la main dans le dossier
        <code> models/</code> de l'app.
      </p>

      <section className="card">
        <h3>ASR — transcription (whisper.cpp)</h3>
        {asr.map((m) => (
          <label
            key={m.id}
            className={m.id === config.asr_model ? "model active" : "model"}
          >
            <input
              type="radio"
              name="asr"
              checked={m.id === config.asr_model}
              onChange={() => save({ ...config, asr_model: m.id })}
            />
            <span className="model-name">{m.name}</span>
            <span className="muted">
              {m.size_mb} Mo · ~{m.ram_gb} Go RAM
              {m.available ? "" : " · à télécharger"}
            </span>
            {m.is_default && <span className="badge">défaut</span>}
          </label>
        ))}
      </section>

      <section className="card">
        <h3>LLM — résumé (llama.cpp)</h3>
        {llm.map((m) => (
          <label
            key={m.id}
            className={m.id === config.llm_model ? "model active" : "model"}
          >
            <input
              type="radio"
              name="llm"
              checked={m.id === config.llm_model}
              onChange={() => save({ ...config, llm_model: m.id })}
            />
            <span className="model-name">{m.name}</span>
            <span className="muted">
              {(m.size_mb / 1024).toFixed(1).replace(".", ",")} Go ·
              ~{m.ram_gb} Go{m.available ? "" : " · à télécharger"}
            </span>
            {m.is_default && <span className="badge">défaut</span>}
          </label>
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
