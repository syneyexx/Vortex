// Experimenten (W17): A/B van prompts/modellen/strategieën, schaduw-run van een nieuwe config,
// metamorfe + counterfactual tests, positionele contexttests, compressie van eigen layers,
// logprob-diagnostiek en (🔒, alleen op commando) ablations.
import { useCallback, useEffect, useState } from "react";
import { api, type Experiment } from "../api/client.ts";
import { setError } from "../state/store.ts";

const fail = (e: unknown) => setError(String(e));

export function ExperimentsPage() {
  const [list, setList] = useState<Experiment[]>([]);
  const [open, setOpen] = useState<string | null>(null);
  const [prompt, setPrompt] = useState("");
  const [model, setModel] = useState("");
  const [diag, setDiag] = useState<Awaited<ReturnType<typeof api.diagnose>> | null>(null);
  const load = useCallback(() => {
    api
      .experiments()
      .then(setList)
      .catch(() => undefined);
  }, []);
  useEffect(() => {
    load();
    const t = setInterval(load, 5000);
    return () => clearInterval(t);
  }, [load]);
  const start = (kind: string, body: Record<string, unknown>) =>
    void api.experiment(kind, body).then(load).catch(fail);
  const variant = {
    name: model ? `model ${model}` : "prompt",
    system_prompt: prompt || undefined,
    model: model || undefined,
  };
  return (
    <div className="page">
      <h1>Experimenten</h1>
      <div className="row" style={{ alignItems: "flex-start", gap: 20, flexWrap: "wrap" }}>
        <div className="panel card" style={{ flex: 1, minWidth: 340 }}>
          <h3>Kandidaat-config</h3>
          <textarea
            placeholder="Nieuwe systeemprompt (leeg = huidige)"
            value={prompt}
            onChange={(e) => setPrompt(e.target.value)}
            style={{ width: "100%", minHeight: 80 }}
          />
          <input
            placeholder="Ander model (naam uit models.toml)"
            value={model}
            onChange={(e) => setModel(e.target.value)}
            style={{ width: "100%", marginTop: 6 }}
          />
          <div className="row" style={{ gap: 6, marginTop: 8, flexWrap: "wrap" }}>
            <button className="btn" onClick={() => start("shadow", { candidate: variant, n: 10 })}>
              Schaduw-run (10 echte vragen)
            </button>
            <button
              className="btn"
              onClick={() => start("ab", { suites: [], a: { name: "huidig" }, b: variant })}
            >
              A/B op de eval-suites
            </button>
          </div>
          <h3 style={{ marginTop: 16 }}>Robuustheid</h3>
          <div className="row" style={{ gap: 6, flexWrap: "wrap" }}>
            <button className="btn" onClick={() => start("metamorphic", { suites: ["redeneren"] })}>
              Metamorf + counterfactual
            </button>
            <button className="btn" onClick={() => start("positional", { model: model || undefined })}>
              Positionele context ({model || "standaardmodel"})
            </button>
            <button className="btn" onClick={() => start("compression", { layer: "router" })}>
              MPO vs int8 (router-layer)
            </button>
            <button className="btn" onClick={() => start("assoc", {})}>
              Associatief geheugen (MRR)
            </button>
            <button className="btn" onClick={() => start("prompt_opt", { suites: [], k: 3 })}>
              Prompt zelf verbeteren
            </button>
            <button
              className="btn"
              title="Grote test — alleen op commando"
              onClick={() => {
                if (
                  window.confirm(
                    "🔒 Ablations zijn een GROTE TEST (lang, alle suites × componenten). Nu starten?",
                  )
                )
                  start("ablation", { confirm: "GROTE TESTEN NU" });
              }}
            >
              🔒 Ablations
            </button>
          </div>
        </div>
        <div className="panel card" style={{ flex: 1, minWidth: 340 }}>
          <h3>Logprob-diagnostiek</h3>
          <div className="row" style={{ gap: 6 }}>
            <input id="diagq" placeholder="Vraag" style={{ flex: 1 }} />
            <button
              className="btn"
              onClick={() => {
                const q = (document.getElementById("diagq") as HTMLInputElement | null)?.value ?? "";
                if (q)
                  void api
                    .diagnose(q, model || undefined)
                    .then(setDiag)
                    .catch(fail);
              }}
            >
              Diagnose
            </button>
          </div>
          {diag && (
            <div style={{ fontSize: 13, marginTop: 8 }}>
              <p>{diag.answer}</p>
              <p style={{ color: "var(--vx-muted)" }}>
                {diag.diagnosis.tokens} tokens · perplexiteit {diag.diagnosis.perplexity.toFixed(2)} ·{" "}
                {(diag.diagnosis.uncertain_share * 100).toFixed(0)}% onzeker
              </p>
              {diag.diagnosis.spans.map((s, i) => (
                <div key={i}>
                  <mark>{s.text}</mark> ({(s.min_prob * 100).toFixed(0)}%)
                  {s.alternatives.length > 0 && ` — alternatief: ${s.alternatives.join(", ")}`}
                </div>
              ))}
            </div>
          )}
        </div>
      </div>
      <div className="panel card" style={{ marginTop: 16 }}>
        <h3>Resultaten</h3>
        <table className="table">
          <thead>
            <tr>
              <th>Soort</th>
              <th>Status</th>
              <th>Gestart</th>
              <th>Samenvatting</th>
            </tr>
          </thead>
          <tbody>
            {list.map((e) => (
              <tr
                key={e.id}
                style={{ cursor: "pointer" }}
                onClick={() => setOpen(open === e.id ? null : e.id)}
              >
                <td>{e.kind}</td>
                <td>
                  <span className={`pill ${e.status === "done" ? "ok" : ""}`}>{e.status}</span>
                </td>
                <td>{e.created_at.slice(0, 16).replace("T", " ")}</td>
                <td style={{ fontSize: 12 }}>
                  {open === e.id ? (
                    <pre style={{ whiteSpace: "pre-wrap", maxHeight: 400, overflow: "auto" }}>
                      {JSON.stringify(e.result, null, 2)}
                    </pre>
                  ) : (
                    summary(e)
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

interface ExpResult {
  error?: string;
  overall?: { diff?: number; ci_low?: number; ci_high?: number; verdict?: string };
  wins?: number;
  losses?: number;
  ties?: number;
  advice?: string;
  suites?: { suite: string; report: { consistency: number; counterfactual_pass: number } }[];
  report?: { weakest?: number };
  result?: { advice?: string };
  components?: { component: string; verdict: string }[];
  advies?: string;
  winner?: unknown;
}

function summary(e: Experiment): string {
  const r = (e.result ?? {}) as ExpResult;
  if (e.status !== "done") return r.error ?? "…";
  const n = (x?: number) => (x ?? 0).toFixed(3);
  switch (e.kind) {
    case "ab":
      return `B − A = ${n(r.overall?.diff)} [${n(r.overall?.ci_low)}, ${n(r.overall?.ci_high)}] → ${r.overall?.verdict ?? "?"}`;
    case "shadow":
      return `${r.wins ?? 0}/${r.losses ?? 0}/${r.ties ?? 0} (winst/verlies/gelijk) → ${r.advice ?? ""}`;
    case "metamorphic":
      return (r.suites ?? [])
        .map(
          (s) =>
            `${s.suite}: consistentie ${(s.report.consistency * 100).toFixed(0)}%, counterfactual ${(s.report.counterfactual_pass * 100).toFixed(0)}%`,
        )
        .join(" · ");
    case "positional":
      return `zwakste positie: ${r.report?.weakest ?? "?"}%`;
    case "compression":
      return r.result?.advice ?? "";
    case "assoc":
      return r.advies ?? "";
    case "prompt_opt":
      return r.winner ? "winnaar gevonden → zie suggesties" : "geen kandidaat won aantoonbaar";
    case "ablation":
      return (r.components ?? []).map((c) => `${c.component}: ${c.verdict}`).join(" · ");
    default:
      return "";
  }
}
