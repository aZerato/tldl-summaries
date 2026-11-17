import { useState } from "react";
import SegmentList from "../components/SegmentList";
import type { PipelineOutput } from "../types";

interface Props {
  result: PipelineOutput;
}

export default function ResultPage({ result }: Props) {
  // Pas de résumé ? On ouvre directement la transcription.
  const [tab, setTab] = useState<"resume" | "transcription">(
    result.summary_markdown ? "resume" : "transcription"
  );
  const [copied, setCopied] = useState(false);

  async function copy() {
    const text =
      tab === "resume"
        ? result.summary_markdown
        : result.transcription.text;
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      // presse-papiers indisponible
    }
  }

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

      {tab === "resume" ? (
        result.summary_markdown ? (
          <pre className="markdown">{result.summary_markdown}</pre>
        ) : (
          <p className="muted">{result.summary_error ?? "Résumé indisponible."}</p>
        )
      ) : (
        <SegmentList segments={result.transcription.segments} />
      )}

      <div className="muted">
        ASR : {result.asr_model} · LLM : {result.llm_model} ·{" "}
        {Math.round(result.duration_secs)} s de média
      </div>
    </div>
  );
}
