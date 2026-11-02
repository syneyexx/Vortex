// Dashboard: scores over tijd per suite (VX:W06.10), eval-runner (VX:W06.01), retrieval-eval,
// lekkagecontrole, foutanalyse en tool/geheugen/model-analytics (VX:W06.09).
import { useCallback, useEffect, useState } from "react";
import { api } from "../api/client.ts";
import type { EvalComparison, EvalResult, EvalRun } from "../api/types.ts";
import { setError, useStore } from "../state/store.ts";

const COLORS = ["#ff5b1f", "#e8e8ec", "#34d27a", "#7aa7ff", "#f2b33d", "#c58cff", "#5fd4d4"];

function ScoreChart({ runs }: { runs: EvalRun[] }) {
  const suites = [...new Set(runs.map((r) => r.suite))].sort();
  const pts = runs
    .filter((r) => r.score !== null && r.finished_at)
    .sort((a, b) => a.started_at.localeCompare(b.started_at));
  if (pts.length === 0) return <div className="empty">Nog geen eval-runs. Start er hieronder één.</div>;
  const t0 = new Date(pts[0]?.started_at ?? 0).getTime();
  const t1 = Math.max(t0 + 1, new Date(pts.at(-1)?.started_at ?? 0).getTime());
  const W = 760;
  const H = 200;
  const x = (r: EvalRun) => 40 + ((new Date(r.started_at).getTime() - t0) / (t1 - t0)) * (W - 60);
  const y = (s: number) => H - 20 - s * (H - 40);
  return (
    <div>
      <svg width={W} height={H} style={{ maxWidth: "100%" }}>
        {[0, 0.5, 1].map((g) => (
          <g key={g}>
            <line x1={40} x2={W - 20} y1={y(g)} y2={y(g)} stroke="#2a2a31" />
            <text x={4} y={y(g) + 4} fill="#8b8a93" fontSize="11">
              {g * 100}%
            </text>
          </g>
        ))}
        {suites.map((s, i) => {
          const rs = pts.filter((r) => r.suite === s);
          const c = COLORS[i % COLORS.length];
          return (
            <g key={s}>
              <polyline
                fill="none"
                stroke={c}
                strokeWidth="1.8"
                points={rs.map((r) => `${x(r)},${y(r.score ?? 0)}`).join(" ")}
              />
              {rs.map((r) => (
                <circle key={r.id} cx={x(r)} cy={y(r.score ?? 0)} r="3" fill={c} />
              ))}
            </g>
          );
        })}
      </svg>
      <div className="row" style={{ flexWrap: "wrap", gap: 12, fontSize: 12 }}>
        {suites.map((s, i) => (
          <span key={s}>
            <i className="dot" style={{ background: COLORS[i % COLORS.length] }} />
            {s}
          </span>
        ))}
      </div>
    </div>
  );
}

function RunDetail({ id }: { id: string }) {
  const [d, setD] = useState<{ comparison: EvalComparison; results: EvalResult[] } | null>(null);
  useEffect(() => {
    api
      .evalRunDetail(id)
      .then(setD)
      .catch((e: unknown) => setError(String(e)));
  }, [id]);
  if (!d) return null;
  const c = d.comparison;
  return (
    <div className="panel card">
      <div className="row">
        <strong>{c.suite}</strong>
        <span
          className={`pill ${c.verdict.startsWith("ROOD") ? "err" : c.verdict.startsWith("GROEN") ? "ok" : ""}`}
        >
          {c.verdict}
        </span>
        <code style={{ color: "var(--vx-muted)", fontSize: 12 }}>{c.current.version}</code>
      </div>
      {c.regressions.length > 0 && (
        <p style={{ color: "var(--vx-err)", fontSize: 13 }}>Achteruit: {c.regressions.join(", ")}</p>
      )}
      {c.improvements.length > 0 && (
        <p style={{ color: "var(--vx-ok)", fontSize: 13 }}>Vooruit: {c.improvements.join(", ")}</p>
      )}
      <table className="table">
        <tbody>
          {d.results.map((r) => (
            <tr key={r.task_id}>
              <td>
                <code>{r.task_id}</code>
              </td>
              <td>
                <span className={`pill ${r.status === "pass" ? "ok" : r.status === "skip" ? "" : "err"}`}>
                  {r.status}
                </span>
              </td>
              <td>{(r.score * 100).toFixed(0)}%</td>
              <td>{r.latency_ms} ms</td>
              <td style={{ fontSize: 12, color: "var(--vx-muted)" }} title={r.output}>
                {r.detail}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

export function DashboardPage() {
  const st = useStore((s) => s.status);
  const seq = useStore((s) => s.lastSeq);
  const [runs, setRuns] = useState<EvalRun[]>([]);
  const [suites, setSuites] = useState<{ name: string; tasks: number }[]>([]);
  const [sel, setSel] = useState<string | null>(null);
  const [an, setAn] = useState<Record<string, unknown>>({});
  const [ret, setRet] = useState<Record<string, unknown> | null>(null);
  const [leak, setLeak] = useState<string | null>(null);
  const load = useCallback(() => {
    api
      .evalRuns()
      .then(setRuns)
      .catch(() => undefined);
    api
      .analytics()
      .then(setAn)
      .catch(() => undefined);
  }, []);
  useEffect(() => {
    api
      .evalSuites()
      .then(setSuites)
      .catch((e: unknown) => setError(String(e)));
  }, []);
  useEffect(load, [load, seq]);
  const pairs = (k: string) => (an[k] as [string, number][] | undefined) ?? [];
  const model = an.model as { avg_decode_tps?: number; avg_ttft_ms?: number } | undefined;
  const memory = an.memory as { retrievals?: number; avg_items?: number } | undefined;
  return (
    <div className="page">
      <h1>Hoe slim is Vortex?</h1>
      <div className="panel card">
        <ScoreChart runs={runs} />
      </div>
      <div className="row" style={{ alignItems: "flex-start", gap: 16 }}>
        <div className="panel card" style={{ flex: 1 }}>
          <h3>Eval draaien</h3>
          <div className="row" style={{ flexWrap: "wrap", gap: 8 }}>
            <button className="btn primary" onClick={() => void api.evalRun("all").then(load)}>
              Alles
            </button>
            {suites.map((s) => (
              <button key={s.name} className="btn" onClick={() => void api.evalRun(s.name).then(load)}>
                {s.name} ({s.tasks})
              </button>
            ))}
          </div>
          <div className="row" style={{ marginTop: 10, gap: 8 }}>
            <button
              className="btn"
              onClick={() =>
                void api
                  .evalRetrieval()
                  .then(setRet)
                  .catch((e: unknown) => setError(String(e)))
              }
            >
              Retrieval-eval
            </button>
            <button
              className="btn"
              onClick={() =>
                void api
                  .evalLeakage()
                  .then((r) =>
                    setLeak(
                      `${r.leaks.length} lek(ken) · ${r.checked_evals} eval-teksten tegen ${r.checked_memories} herinneringen`,
                    ),
                  )
              }
            >
              Lekkagecontrole
            </button>
          </div>
          {ret && (
            <p style={{ fontSize: 13 }}>
              recall@1 {(Number(ret.recall_at_1) * 100).toFixed(0)}% · @3{" "}
              {(Number(ret.recall_at_3) * 100).toFixed(0)}% · @5 {(Number(ret.recall_at_5) * 100).toFixed(0)}%
              · MRR {Number(ret.mrr).toFixed(2)} · irrelevant@5{" "}
              {(Number(ret.irrelevant_at_5) * 100).toFixed(0)}% · geen-antwoord{" "}
              {(Number(ret.no_answer_correct) * 100).toFixed(0)}%
            </p>
          )}
          {leak && <p style={{ fontSize: 13 }}>{leak}</p>}
          <table className="table" style={{ marginTop: 10 }}>
            <tbody>
              {runs.slice(0, 15).map((r) => (
                <tr key={r.id} style={{ cursor: "pointer" }} onClick={() => setSel(r.id)}>
                  <td>{r.suite}</td>
                  <td>{r.finished_at ? `${r.passed}/${r.n}` : "bezig…"}</td>
                  <td>{r.score !== null ? `${(r.score * 100).toFixed(0)}%` : "—"}</td>
                  <td style={{ color: "var(--vx-muted)", fontSize: 12 }}>
                    {new Date(r.started_at).toLocaleString("nl-NL")}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        <div className="panel card" style={{ width: 460 }}>
          <h3>Analytics</h3>
          <table className="table">
            <tbody>
              <tr>
                <th>Runs</th>
                <td>
                  {pairs("runs_by_state")
                    .map(([k, v]) => `${k} ${v}`)
                    .join(" · ") || "—"}
                </td>
              </tr>
              <tr>
                <th>Feedback</th>
                <td>
                  {pairs("feedback")
                    .map(([k, v]) => `${k} ${v}`)
                    .join(" · ") || "—"}
                </td>
              </tr>
              <tr>
                <th>Model</th>
                <td>
                  {model?.avg_decode_tps ? `${model.avg_decode_tps.toFixed(1)} t/s` : "—"} · TTFT{" "}
                  {model?.avg_ttft_ms ? `${model.avg_ttft_ms.toFixed(0)} ms` : "—"}
                </td>
              </tr>
              <tr>
                <th>Geheugen</th>
                <td>
                  {memory?.retrievals ?? 0} keer opgehaald · gem. {memory?.avg_items?.toFixed(1) ?? "—"} items
                </td>
              </tr>
              <tr>
                <th>Tools</th>
                <td>
                  {pairs("tools")
                    .map(([k, v]) => `${k} ${v}`)
                    .join(" · ") || "nog geen (W08)"}
                </td>
              </tr>
              <tr>
                <th>Stalls</th>
                <td>{String(an.stalls ?? 0)}</td>
              </tr>
              <tr>
                <th>Subsystemen</th>
                <td>
                  {st?.subsystems.map((s) => (
                    <span key={s.name} className={`pill ${s.ok ? "ok" : "err"}`} style={{ marginRight: 4 }}>
                      {s.name}
                    </span>
                  ))}
                </td>
              </tr>
            </tbody>
          </table>
          <h3>Meest voorkomende fouten</h3>
          {pairs("top_errors").map(([k, v]) => (
            <div key={k} style={{ fontSize: 13, padding: "4px 0" }}>
              {v}× {k}
            </div>
          ))}
          {pairs("top_errors").length === 0 && (
            <p style={{ color: "var(--vx-muted)", fontSize: 13 }}>Geen mislukte runs.</p>
          )}
        </div>
      </div>
      {sel && <RunDetail key={sel} id={sel} />}
    </div>
  );
}
