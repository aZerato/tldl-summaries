import { useRef, useState } from "react";
import SegmentList from "../components/SegmentList";
import { convertFileSrc } from "../api/tauri";
import type { PipelineOutput } from "../types";

interface Props {
  result: PipelineOutput;
}

export default function ResultPage({ result }: Props) {
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

      {tab === "resume" ? (
        result.summaryMarkdown ? (
          <pre className="markdown">{result.summaryMarkdown}</pre>
        ) : (
          <p className="muted">{result.summaryError ?? "Résumé indisponible."}</p>
        )
      ) : (
        <SegmentList
          segments={segments}
          onSeek={seekTo}
          activeIndex={activeIndex}
        />
      )}

      {result.deviceNote && (
        <div className="error" role="alert">
          {result.deviceNote}
        </div>
      )}

      <div className="muted">
        ASR : {result.asrModel} · LLM : {result.llmModel} · Device :{" "}
        {result.device} · {Math.round(result.durationSecs)} s de média
      </div>
    </div>
  );
}
