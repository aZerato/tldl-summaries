import type { Segment } from "../types";

interface Props {
  segments: Segment[];
  /** F13 : lecture du segment (seek + play) — si absent, pas cliquables. */
  onSeek?: (t: number) => void;
  /** F13 : index du segment en cours de lecture (-1 = aucun). */
  activeIndex?: number;
}

function fmt(secs: number): string {
  const m = Math.floor(secs / 60);
  const s = Math.floor(secs % 60);
  return `${m}:${s.toString().padStart(2, "0")}`;
}

export default function SegmentList({
  segments,
  onSeek,
  activeIndex = -1,
}: Props) {
  if (segments.length === 0) return <p className="muted">Aucun segment.</p>;
  const interactive = onSeek !== undefined;
  // F9 : une teinte par intervenante (au fil de la transcription).
  const colorOf = new Map<string, string>();
  for (const s of segments) {
    if (s.speaker && !colorOf.has(s.speaker)) {
      colorOf.set(
        s.speaker,
        `var(--spk-${colorOf.size % 4})`,
      );
    }
  }
  return (
    <ul className="segments">
      {segments.map((s, i) => (
        <li
          key={i}
          className={
            [
              "segment",
              interactive ? "clickable" : "",
              i === activeIndex ? "active" : "",
            ]
              .filter(Boolean)
              .join(" ")
          }
          role={interactive ? "button" : undefined}
          tabIndex={interactive ? 0 : undefined}
          title={interactive ? "Jouer à partir de ce segment" : undefined}
          onClick={interactive ? () => onSeek(s.start) : undefined}
          onKeyDown={
            interactive
              ? (e) => {
                  if (e.key === "Enter") onSeek(s.start);
                }
              : undefined
          }
        >
          <span className="segment-time">{fmt(s.start)}</span>
          {s.speaker && (
            <span
              className="segment-speaker"
              style={{ color: colorOf.get(s.speaker) }}
            >
              {s.speaker}
            </span>
          )}
          <span>{s.text}</span>
        </li>
      ))}
    </ul>
  );
}
