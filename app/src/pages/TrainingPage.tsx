// Training & evals (W13.09): datasets (SFT/DPO/Q&A), adapters (status, scores per suite,
// regressies, schaduwrun), poortwachter, hot-swap, kalibratie van de Judge-zekerheid.
import { useCallback, useEffect, useState } from "react";
import { api, type TrainingStatus } from "../api/client.ts";
import { setError } from "../state/store.ts";

const pct = (x: number | undefined) => (x === undefined ? "–" : `${(x * 100).toFixed(1)}%`);

export function TrainingPage() {
  const [st, setSt] = useState<TrainingStatus | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  const load = useCallback(() => {
    api
      .training()
      .then(setSt)
      .catch(() => undefined);
  }, []);
  useEffect(load, [load]);
  const run = (key: string, p: Promise<unknown>, ok: (r: unknown) => string) => {
    setBusy(key);
    p.then((r) => setMsg(ok(r)))
      .catch((e: unknown) => setError(String(e)))
      .finally(() => {
        setBusy(null);
        load();
      });
  };
  if (!st) return null;
  return (
    <div className="page">
      <h1>Training &amp; evals</h1>
      <p style={{ color: "var(--vx-muted)", fontSize: 13 }}>
        Basismodel: <code>{st.config.base_model || "niet ingesteld (config/training.toml)"}</code> · training{" "}
        {st.config.idle ? "alleen in idle" : "altijd"} · automatisch bij ≥ {st.config.min_new_examples} nieuwe
        voorbeelden. Een adapter wordt pas actief als hij op de eval wint zonder regressie elders.
      </p>
      {msg && (
        <div className="pill ok" style={{ marginBottom: 12 }}>
          {msg}
        </div>
      )}
      <div className="row" style={{ alignItems: "flex-start", gap: 20 }}>
        <div className="panel card" style={{ flex: 1 }}>
          <div className="row" style={{ justifyContent: "space-between" }}>
            <h3>Datasets</h3>
            <div className="row" style={{ gap: 6 }}>
              {(["sft", "dpo", "qa"] as const).map((k) => (
                <button
                  key={k}
                  className="btn"
                  disabled={!!busy}
                  onClick={() =>
                    run(`ds:${k}`, api.buildDataset(k), (r) => {
                      const x = r as { n: number; decontaminated: number };
                      return `${k.toUpperCase()}: ${x.n} voorbeelden (${x.decontaminated} verwijderd: overlap met eval)`;
                    })
                  }
                >
                  {busy === `ds:${k}` ? "bezig…" : `+ ${k.toUpperCase()}`}
                </button>
              ))}
            </div>
          </div>
          <table className="table">
            <tbody>
              {st.datasets.map((d) => (
                <tr key={d.version}>
                  <td>
                    <code>{d.version}</code>
                  </td>
                  <td>{d.n}</td>
                  <td style={{ fontSize: 12 }}>
                    {Object.entries(d.stats ?? {})
                      .map(([k, v]) => `${k}: ${v}`)
                      .join(" · ")}
                  </td>
                  <td>
                    {(d.kind === "sft" || d.kind === "dpo") && (
                      <button
                        className="btn"
                        disabled={!!busy}
                        onClick={() =>
                          run(
                            `tr:${d.version}`,
                            api.trainAdapter(d.kind, d.version),
                            () => "Training gestart/afgerond — zie adapters",
                          )
                        }
                      >
                        trainen
                      </button>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        <div className="panel card" style={{ width: 380 }}>
          <div className="row" style={{ justifyContent: "space-between" }}>
            <h3>Kalibratie</h3>
            <button
              className="btn"
              onClick={() => run("cal", api.calibrate(), () => "Kalibrator opnieuw gefit")}
            >
              fitten
            </button>
          </div>
          {st.calibrator ? (
            <p style={{ fontSize: 13 }}>
              Judge-zekerheid op aparte testset (n={st.calibrator.test_raw.n}): ECE{" "}
              {st.calibrator.test_raw.ece.toFixed(3)} → {st.calibrator.test_calibrated.ece.toFixed(3)} · Brier{" "}
              {st.calibrator.test_raw.brier.toFixed(3)} → {st.calibrator.test_calibrated.brier.toFixed(3)}
            </p>
          ) : (
            <div className="empty">Nog niet gefit (minstens 10 beoordeelde antwoorden nodig).</div>
          )}
        </div>
      </div>
      <div className="panel card">
        <h3>Adapters</h3>
        {st.adapters.length === 0 && <div className="empty">Nog geen adapters.</div>}
        <table className="table">
          <tbody>
            {st.adapters.map((a) => (
              <tr key={a.id}>
                <td>
                  <code>{a.id}</code>
                  <div style={{ fontSize: 12, color: "var(--vx-muted)" }}>
                    {a.kind} · {a.dataset}
                  </div>
                </td>
                <td>
                  <span
                    className={`pill ${a.status === "active" ? "ok" : a.status === "rejected" || a.status === "failed" ? "err" : "run"}`}
                  >
                    {a.status}
                  </span>
                  {a.status === "failed" && (
                    <div style={{ fontSize: 12 }}>{String(a.metrics.error ?? "")}</div>
                  )}
                </td>
                <td style={{ fontSize: 12 }}>
                  {Object.entries(a.scores).map(([s, v]) => (
                    <div key={s}>
                      {s}: {pct(v.base)} → {pct(v.adapter)}
                    </div>
                  ))}
                  {a.regressions.length > 0 && (
                    <div style={{ color: "var(--vx-err)" }}>
                      regressie: {a.regressions.map((r) => r.suite).join(", ")}
                    </div>
                  )}
                </td>
                <td style={{ fontSize: 12 }}>
                  {a.shadow.n ? `schaduw ${a.shadow.n}× · winst ${pct(a.shadow.win_rate)}` : ""}
                </td>
                <td className="row" style={{ gap: 4 }}>
                  {["trained", "shadow", "active", "rejected"].includes(a.status) && (
                    <>
                      <button
                        className="btn"
                        disabled={!!busy}
                        onClick={() =>
                          run(
                            `sh:${a.id}`,
                            api.shadowAdapter(a.id),
                            (r) => `Schaduwrun: ${pct((r as { win_rate: number }).win_rate)} gewonnen`,
                          )
                        }
                      >
                        schaduw
                      </button>
                      <button
                        className="btn"
                        disabled={!!busy}
                        onClick={() =>
                          run(
                            `g:${a.id}`,
                            api.gateAdapter(a.id),
                            (r) => `Poortwachter: ${(r as { status: string }).status}`,
                          )
                        }
                      >
                        poortwachter
                      </button>
                      <button
                        className="btn"
                        onClick={() =>
                          run(
                            `s:${a.id}`,
                            api.scaleAdapter(a.id, a.status === "active" ? 0 : 1),
                            () => "Adapter gewisseld",
                          )
                        }
                      >
                        {a.status === "active" ? "uit" : "aan"}
                      </button>
                    </>
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
