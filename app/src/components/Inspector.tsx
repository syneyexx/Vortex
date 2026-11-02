// Context Assembly Inspector + trace-viewer voor één run (VX:W06.05, VX:W06.06).
import { useEffect, useState } from "react";
import { api } from "../api/client.ts";
import type { TraceResp } from "../api/types.ts";
import { setError } from "../state/store.ts";

export function Inspector({ runId, onClose }: { runId: string; onClose: () => void }) {
  const [t, setT] = useState<TraceResp | null>(null);
  const [tab, setTab] = useState<"context" | "trace">("context");
  useEffect(() => {
    api
      .trace(runId)
      .then(setT)
      .catch((e: unknown) => setError(String(e)));
  }, [runId]);
  const ctx = t?.events.find((e) => e.kind === "context.prompt.assembled")?.payload as
    | {
        config_version?: string;
        messages?: number;
        history_messages?: number;
        approx_tokens?: number;
        dropped_messages?: number;
        memories?: { id: string; kind: string; score: number; counter: boolean }[];
        system_prompt_chars?: number;
      }
    | undefined;
  const ret = t?.events.find((e) => e.kind === "memory.retrieval.performed")?.payload as
    | {
        items?: {
          id: string;
          kind: string;
          counter: boolean;
          score: Record<string, number | null | boolean>;
        }[];
        candidates?: number;
        vector_stage?: boolean;
      }
    | undefined;
  const inf = t?.inference[0];
  const total = Math.max(1, t?.spans.total_ms ?? 1);
  return (
    <div className="modal-back" onClick={onClose}>
      <div className="panel modal" onClick={(e) => e.stopPropagation()}>
        <div className="row">
          <h2 style={{ margin: 0, fontWeight: 500 }}>Inspector</h2>
          <code style={{ color: "var(--vx-muted)" }}>{runId}</code>
          <button className="btn" style={{ marginLeft: "auto" }} onClick={onClose}>
            Sluiten
          </button>
        </div>
        <div className="chat-tabs" style={{ display: "inline-flex", margin: "12px 0" }}>
          <button className={tab === "context" ? "active" : ""} onClick={() => setTab("context")}>
            Context
          </button>
          <button className={tab === "trace" ? "active" : ""} onClick={() => setTab("trace")}>
            Trace
          </button>
        </div>
        {!t && <div className="empty">Laden…</div>}
        {t && tab === "context" && (
          <div>
            {ctx ? (
              <table className="table">
                <tbody>
                  <tr>
                    <th>Config-versie</th>
                    <td>
                      <code>{ctx.config_version}</code>
                    </td>
                  </tr>
                  <tr>
                    <th>System prompt</th>
                    <td>{ctx.system_prompt_chars ?? 0} tekens (incl. herinneringen)</td>
                  </tr>
                  <tr>
                    <th>Berichten in context</th>
                    <td>
                      {ctx.messages} ({ctx.history_messages} gesprek)
                    </td>
                  </tr>
                  <tr>
                    <th>Geschat</th>
                    <td>~{ctx.approx_tokens} tokens</td>
                  </tr>
                  <tr>
                    <th>Weggevallen</th>
                    <td>{ctx.dropped_messages ?? 0} berichten (compactie volgt in W07)</td>
                  </tr>
                </tbody>
              </table>
            ) : (
              <div className="empty">Geen context-event voor deze run.</div>
            )}
            <h3>Herinneringen — waarom opgehaald?</h3>
            {ret?.vector_stage === false && (
              <p className="pill err">Embeddingmodel niet bereikbaar: zonder vectorstap gezocht.</p>
            )}
            {(ret?.items ?? []).map((it) => (
              <div
                key={it.id}
                style={{ padding: "8px 0", borderBottom: "1px solid var(--vx-border)", fontSize: 13 }}
              >
                <span className="pill">{it.kind}</span> <code>{it.id.slice(0, 16)}…</code>{" "}
                {it.counter && <span className="pill err">tegenbewijs</span>}
                <div style={{ color: "var(--vx-muted)", marginTop: 4 }}>
                  {Object.entries(it.score)
                    .filter(([, v]) => v !== null && v !== false)
                    .map(([k, v]) => `${k}: ${typeof v === "number" ? v.toFixed(3) : String(v)}`)
                    .join(" · ")}
                </div>
              </div>
            ))}
            {!ret?.items?.length && (
              <p style={{ color: "var(--vx-muted)" }}>Niets opgehaald ({ret?.candidates ?? 0} kandidaten).</p>
            )}
          </div>
        )}
        {t && tab === "trace" && (
          <div>
            <div className="row" style={{ gap: 8, flexWrap: "wrap" }}>
              <span className="pill">totaal {t.spans.total_ms} ms</span>
              <span className="pill">wachten {t.spans.wait_ms ?? "—"} ms</span>
              <span className="pill">uitvoeren {t.spans.execute_ms ?? "—"} ms</span>
              {inf && (
                <>
                  <span className="pill">TTFT {inf.ttft_ms?.toFixed(0) ?? "—"} ms</span>
                  <span className="pill">prefill {inf.prefill_tps?.toFixed(0) ?? "—"} t/s</span>
                  <span className="pill">decode {inf.decode_tps?.toFixed(1) ?? "—"} t/s</span>
                  <span className="pill">{inf.completion_tokens ?? 0} tokens</span>
                </>
              )}
            </div>
            <div style={{ marginTop: 12, maxHeight: "55vh", overflow: "auto" }}>
              {t.events.map((e) => (
                <div key={e.seq} className="trace-row">
                  <span className="trace-t">{e.t_ms} ms</span>
                  <span className="trace-bar">
                    <i style={{ left: `${(100 * e.t_ms) / total}%` }} />
                  </span>
                  <span className="trace-k">{e.kind}</span>
                </div>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
