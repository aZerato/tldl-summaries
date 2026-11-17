import type { Segment } from "../types";

function fmt(secs: number): string {
  const m = Math.floor(secs / 60);
  const s = Math.floor(secs % 60);
  return `${m}:${s.toString().padStart(2, "0")}`;
}

export default function SegmentList({ segments }: { segments: Segment[] }) {
  if (segments.length === 0) return <p className="muted">Aucun segment.</p>;
  return (
    <ul className="segments">
      {segments.map((s, i) => (
        <li key={i} className="segment">
          <span className="segment-time">{fmt(s.start)}</span>
          <span>{s.text}</span>
        </li>
      ))}
    </ul>
  );
}
