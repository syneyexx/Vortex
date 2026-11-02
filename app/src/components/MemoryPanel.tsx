// Retrieved memory-paneel (links): wat Vortex voor het laatste antwoord uit het geheugen haalde,
// met score en herkomst. Live via het retrieval-event van de run. VX:W05.13
import { useEffect, useState } from "react";
import { api } from "../api/client.ts";
import type { Memory, RetrievalItem } from "../api/types.ts";
import { useStore } from "../state/store.ts";
import { IcBook, IcClock, IcDb, IcUser } from "./Icons.tsx";

const EMPTY: RetrievalItem[] = [];
const cache = new Map<string, Memory>();

function iconFor(kind: string) {
  if (kind === "episodic") return IcClock;
  if (kind === "procedural") return IcDb;
  if (kind === "user") return IcUser;
  return IcBook;
}

function label(kind: string): string {
  const m: Record<string, string> = {
    semantic: "Semantic",
    episodic: "Episodic",
    procedural: "Procedural",
    user: "User",
    project: "Project",
    summary: "Summary",
    world: "World",
    long_term: "Long-term",
    short_term: "Short-term",
  };
  return m[kind] ?? kind;
}

function chip(m: Memory | undefined): string {
  if (!m) return "…";
  const src = m.source_uri ?? m.provenance.origin ?? m.provenance.source;
  if (src.startsWith("chat:")) return "Session";
  if (src.startsWith("file:")) return "Notes";
  if (m.provenance.source === "tool") return "Tool";
  return m.kind === "procedural" ? "Workflow" : "Memory";
}

export function MemoryPanel() {
  const items = useStore((s) => {
    const list = s.currentSession ? s.messages[s.currentSession] : undefined;
    const last = list?.filter((m) => m.role === "assistant").at(-1);
    return (last?.run_id ? s.retrievals[last.run_id] : undefined) ?? EMPTY;
  });
  const [, force] = useState(0);
  useEffect(() => {
    let alive = true;
    void Promise.all(
      items
        .filter((i) => !cache.has(i.id))
        .map((i) =>
          api
            .memory(i.id)
            .then((r) => cache.set(i.id, r.memory))
            .catch(() => undefined),
        ),
    ).then(() => alive && force((x) => x + 1));
    return () => {
      alive = false;
    };
  }, [items]);
  const top = items.slice(0, 3);
  return (
    <section className="panel memory">
      <div>
        <h2 className="section-title">Retrieved memory</h2>
        {top.length === 0 && (
          <div style={{ color: "var(--vx-muted)", fontSize: 13, padding: "4px 0" }}>
            Nog niets opgehaald voor dit gesprek.
          </div>
        )}
        {top.map((it) => {
          const m = cache.get(it.id);
          const I = iconFor(it.kind);
          const title = m ? (m.title || m.content).slice(0, 40) : "…";
          const sp = it.score;
          const why = [
            sp.vector != null ? `vector ${(sp.vector * 100).toFixed(0)}%` : null,
            sp.bm25_rank != null ? `trefwoord #${sp.bm25_rank}` : null,
            sp.graph_hops != null ? `graph ${sp.graph_hops} hop` : null,
            sp.rerank != null ? `rerank ${(sp.rerank * 100).toFixed(0)}%` : null,
            it.counter ? "tegenbewijs" : null,
          ]
            .filter(Boolean)
            .join(" · ");
          return (
            <div className="mem-row" key={it.id} title={`Waarom opgehaald: ${why}`}>
              <I size={20} />
              <span>
                {label(it.kind)} <span style={{ color: "var(--vx-muted)" }}>· {title}</span>
                {!it.counter && <span className="score"> · {(sp.final_score * 100).toFixed(0)}%</span>}
                {it.counter && <span style={{ color: "var(--vx-warn)" }}> · tegenbewijs</span>}
              </span>
              <span className="chip">{chip(m)}</span>
            </div>
          );
        })}
      </div>
      <div style={{ display: "grid", alignContent: "center", justifyItems: "center", gap: 8 }}>
        <MiniGraph n={items.length} />
        <div className="graph-cap">Knowledge graph</div>
      </div>
    </section>
  );
}

function MiniGraph({ n }: { n: number }) {
  const pts = Array.from({ length: Math.min(n, 7) }, (_, i) => {
    const a = (i / Math.max(1, Math.min(n, 7))) * Math.PI * 2 - Math.PI / 2;
    return [55 + Math.cos(a) * 38, 50 + Math.sin(a) * 38] as const;
  });
  return (
    <svg width="110" height="100" viewBox="0 0 110 100" aria-label={`Knowledge graph (${n} items)`}>
      {pts.map(([x, y], i) => (
        <line key={`l${i}`} x1="55" y1="50" x2={x} y2={y} stroke="#3a3a42" strokeWidth="1" />
      ))}
      {pts.map(([x, y], i) => (
        <circle key={`c${i}`} cx={x} cy={y} r="5" fill="#d9d9de" />
      ))}
      <circle
        cx="55"
        cy="50"
        r={n ? 8 : 7}
        fill={n ? "#ff5b1f" : "none"}
        stroke={n ? "none" : "#3a3a42"}
        style={n ? { filter: "drop-shadow(0 0 6px rgba(255,91,31,.7))" } : undefined}
      />
    </svg>
  );
}
