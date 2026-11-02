// Research-pagina (W10.11): onderzoek starten (quick/standaard/deep), live deelvragen, bronnen
// met betrouwbaarheid, binnenkomende records, rapport met bronverwijzingen, export CSV/JSONL/Parquet;
// plus globale corpus-synthese over alle ingelezen documenten.
import { useCallback, useEffect, useState } from "react";
import { api, type ResearchDetail } from "../api/client.ts";
import { Markdown } from "../components/Markdown.tsx";
import { setError, useStore } from "../state/store.ts";

function recordText(kind: string, d: Record<string, unknown>): string {
  const g = (k: string) => (d[k] === undefined || d[k] === null ? "" : String(d[k]));
  switch (kind) {
    case "claim":
      return g("statement");
    case "number":
      return `${g("what")}: ${g("value")} ${g("unit")}${g("when") ? ` (${g("when")})` : ""}`;
    case "entity":
      return `${g("name")} (${g("type")})`;
    case "relation":
      return `${g("subject")} ${g("predicate")} ${g("object")}`;
    case "event":
      return `${g("what")}${g("when") ? ` — ${g("when")}` : ""}`;
    case "quote":
      return `"${g("text")}"${g("speaker") ? ` — ${g("speaker")}` : ""}`;
    default:
      return JSON.stringify(d);
  }
}

export function ResearchPage() {
  const [list, setList] = useState<Awaited<ReturnType<typeof api.researchList>>>([]);
  const [sel, setSel] = useState<string | null>(null);
  const [d, setD] = useState<ResearchDetail | null>(null);
  const [q, setQ] = useState("");
  const [mode, setMode] = useState("standard");
  const [corpus, setCorpus] = useState<string | null>(null);
  const lastSeq = useStore((s) => s.lastSeq);

  const loadList = useCallback(() => {
    api
      .researchList()
      .then(setList)
      .catch((e: unknown) => setError(String(e)));
  }, []);
  useEffect(loadList, [loadList]);
  // Live: bij nieuwe events (research.*) het geselecteerde onderzoek verversen.
  useEffect(() => {
    if (!sel) return;
    const t = setTimeout(() => {
      api
        .research(sel)
        .then(setD)
        .catch(() => undefined);
    }, 300);
    return () => clearTimeout(t);
  }, [sel, lastSeq]);

  const start = async () => {
    if (q.trim().length < 3) return;
    const r = await api.startResearch(q.trim(), mode);
    setQ("");
    setSel(r.id);
    loadList();
  };

  return (
    <div className="page research">
      <h1>Research</h1>
      <div className="panel card">
        <div className="row">
          <input
            className="btn"
            style={{ flex: 1 }}
            placeholder="Onderzoeksvraag…"
            value={q}
            onChange={(e) => setQ(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && void start()}
          />
          <select className="btn" value={mode} onChange={(e) => setMode(e.target.value)}>
            <option value="quick">Quick lookup</option>
            <option value="standard">Standaard</option>
            <option value="deep">Deep research</option>
          </select>
          <button className="btn primary" onClick={() => void start()}>
            Start
          </button>
          <button
            className="btn"
            title="Vraag over alle ingelezen documenten samen"
            onClick={() =>
              q.trim() &&
              void api
                .corpus(q.trim())
                .then((r) => setCorpus(r.answer))
                .catch((e: unknown) => setError(String(e)))
            }
          >
            Hele verzameling
          </button>
        </div>
        {corpus && (
          <div style={{ marginTop: 12 }}>
            <h3>Over de hele verzameling</h3>
            <Markdown text={corpus} streaming={false} />
          </div>
        )}
      </div>
      <div className="row" style={{ alignItems: "flex-start", gap: 20 }}>
        <div className="panel card" style={{ width: 320 }}>
          <h3>Onderzoeken</h3>
          {list.length === 0 && <div className="empty">Nog geen onderzoek.</div>}
          {list.map((r) => (
            <div
              key={r.id}
              className={`research-item ${sel === r.id ? "active" : ""}`}
              onClick={() => setSel(r.id)}
            >
              <div>{r.question}</div>
              <div style={{ fontSize: 12, color: "var(--vx-muted)" }}>
                <span className={`pill ${r.status === "klaar" ? "ok" : "run"}`}>{r.status}</span> {r.mode} ·{" "}
                {new Date(r.created_at).toLocaleString("nl-NL")}
              </div>
            </div>
          ))}
        </div>
        {d && (
          <div style={{ flex: 1, minWidth: 0 }}>
            <div className="panel card">
              <div className="row" style={{ justifyContent: "space-between" }}>
                <h2 style={{ margin: 0 }}>{d.question}</h2>
                <span className={`pill ${d.status === "klaar" ? "ok" : "run"}`}>{d.status}</span>
              </div>
              <h3>Deelvragen</h3>
              {d.subquestions.map((s) => (
                <div key={s.i} className="skill-row">
                  <div>
                    <span
                      className={`pill ${s.status === "answered" ? "ok" : s.status === "open" ? "run" : "err"}`}
                    >
                      {s.status}
                    </span>{" "}
                    {s.text}
                  </div>
                  {s.answer && <div style={{ fontSize: 13, color: "var(--vx-text-2)" }}>{s.answer}</div>}
                </div>
              ))}
            </div>
            <div className="panel card">
              <h3>Bronnen ({d.sources.length})</h3>
              <table className="table">
                <tbody>
                  {d.sources.map((s) => (
                    <tr key={s.n}>
                      <td>[{s.n}]</td>
                      <td>
                        <span className="pill">{s.kind}</span>
                      </td>
                      <td>
                        {s.uri.startsWith("http") ? (
                          <a href={s.uri} target="_blank" rel="noreferrer">
                            {s.title || s.uri}
                          </a>
                        ) : (
                          s.title || s.uri
                        )}
                        {s.locator && <span style={{ color: "var(--vx-muted)" }}> · {s.locator}</span>}
                      </td>
                      <td title="betrouwbaarheid">{(s.reliability * 100).toFixed(0)}%</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            <div className="panel card">
              <div className="row" style={{ justifyContent: "space-between" }}>
                <h3>Records ({d.records.length})</h3>
                <div className="row" style={{ gap: 6 }}>
                  {["csv", "jsonl", "parquet"].map((f) => (
                    <a key={f} className="btn" href={`/api/research/${d.id}/export?format=${f}`}>
                      {f.toUpperCase()}
                    </a>
                  ))}
                </div>
              </div>
              <table className="table">
                <tbody>
                  {d.records.map((r) => (
                    <tr key={r.id}>
                      <td>
                        <span className="pill">{r.kind}</span>
                      </td>
                      <td>{recordText(r.kind, r.data)}</td>
                      <td>
                        <span className={`pill ${r.status === "supported" ? "ok" : ""}`}>
                          {r.status === "supported" ? "gestaafd" : "onbevestigd"}
                        </span>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            {d.report && (
              <div className="panel card">
                <h3>Rapport</h3>
                {d.report_path && (
                  <div style={{ fontSize: 12, color: "var(--vx-muted)" }}>{d.report_path}</div>
                )}
                <Markdown text={d.report} streaming={false} />
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
