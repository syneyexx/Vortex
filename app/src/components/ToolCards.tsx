// Tool-kaarten in de chat (W08.15): live per run uit tool.call.*-events. Klik = details
// (invoer, preview/diff, uitvoer of fout).
import { useState } from "react";
import { useStore } from "../state/store.ts";
import type { ToolCard } from "../state/reduce.ts";
import { IcCheck, IcCode, IcDoc, IcGlobe, IcSearch } from "./Icons.tsx";

function icon(tool: string) {
  if (tool.includes("search") || tool.includes("find")) return <IcSearch size={22} />;
  if (tool.startsWith("web") || tool.startsWith("http") || tool.startsWith("api__"))
    return <IcGlobe size={22} />;
  if (tool === "python" || tool === "shell" || tool.includes("script")) return <IcCode size={22} />;
  return <IcDoc size={22} />;
}

function label(t: ToolCard): string {
  const i = (t.input ?? {}) as Record<string, unknown>;
  const target = ["path", "query", "url", "pattern", "name", "db", "command"]
    .map((k) => i[k])
    .find((v) => typeof v === "string");
  return typeof target === "string" ? target.slice(0, 80) : "";
}

const EMPTY: ToolCard[] = [];

export function ToolCards({ runId }: { runId: string }) {
  const cards = useStore((s) => s.tools[runId] ?? EMPTY);
  const [open, setOpen] = useState<string | null>(null);
  if (cards.length === 0) return null;
  const sel = cards.find((c) => c.id === open);
  return (
    <>
      <div className="tool-cards">
        {cards.map((c) => (
          <button
            className={`tool-card ${c.status}`}
            key={c.id}
            onClick={() => setOpen(open === c.id ? null : c.id)}
            title={c.error ?? c.output ?? ""}
          >
            {icon(c.tool)}
            <div style={{ minWidth: 0 }}>
              <div className="t">{c.tool}</div>
              <div
                className="d"
                style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}
              >
                {c.status === "running"
                  ? "bezig… "
                  : c.status === "error"
                    ? `fout: ${c.error ?? ""}`
                    : `${c.ms ?? 0} ms${c.replayed ? " · al gedaan" : ""}${c.cached ? " · cache" : ""} `}
                {label(c)}
              </div>
            </div>
            {c.status === "ok" ? (
              <IcCheck color="var(--vx-ok)" />
            ) : c.status === "error" ? (
              <span style={{ color: "var(--vx-err)" }}>!</span>
            ) : (
              <i className="dot accent" />
            )}
          </button>
        ))}
      </div>
      {sel && (
        <div className="panel card tool-detail">
          <div className="row" style={{ justifyContent: "space-between" }}>
            <strong>{sel.tool}</strong>
            <button className="btn" onClick={() => setOpen(null)}>
              sluiten
            </button>
          </div>
          {sel.input !== undefined && <pre>{JSON.stringify(sel.input, null, 2)}</pre>}
          {sel.preview && (
            <>
              <h4>Wijziging</h4>
              <pre className="diff">
                {sel.preview.split("\n").map((l, k) => (
                  <span
                    key={k}
                    className={
                      l.startsWith("+") && !l.startsWith("+++")
                        ? "add"
                        : l.startsWith("-") && !l.startsWith("---")
                          ? "del"
                          : ""
                    }
                  >
                    {l + "\n"}
                  </span>
                ))}
              </pre>
            </>
          )}
          {sel.output && (
            <>
              <h4>Uitvoer</h4>
              <pre>{sel.output}</pre>
            </>
          )}
          {sel.error && <div className="msg-error">{sel.error}</div>}
        </div>
      )}
    </>
  );
}
