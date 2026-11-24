import type { Progress } from "../types";

const STEPS: Record<string, string> = {
  probe: "Lecture du fichier…",
  convert: "Extraction de la piste audio…",
  asr: "Transcription…",
  diar: "Séparation des intervenants…",
  llm: "Rédaction du résumé…",
  done: "Terminé.",
};

export default function Progress({ progress }: { progress: Progress }) {
  const pct =
    progress.total > 0
      ? Math.min(100, Math.max(0, (progress.position / progress.total) * 100))
      : 0;
  return (
    <div className="progress">
      <div className="progress-label">{STEPS[progress.step] ?? progress.step}</div>
      <div className="progress-bar">
        <div style={{ width: `${pct}%` }} />
      </div>
    </div>
  );
}
