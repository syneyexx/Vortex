// Dashboard: neurale layers (W11.09) — modus, versies, eval (net vs heuristiek), schaduw-
// overeenstemming, voorbeelden; acties: bootstrap-labels, trainen, promoveren, terugdraaien.
import { useCallback, useEffect, useState } from "react";
import { api, type NeuralStatus } from "../api/client.ts";
import { setError } from "../state/store.ts";

const pct = (x: number | null | undefined) =>
  x === null || x === undefined ? "–" : `${(x * 100).toFixed(1)}%`;

export function NeuralPanel() {
  const [st, setSt] = useState<NeuralStatus | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  const load = useCallback(() => {
    api
      .neural()
      .then(setSt)
      .catch(() => undefined);
  }, []);
  useEffect(load, [load]);
  const act = (layer: string, a: "train" | "bootstrap" | "promote" | "rollback") => {
    setBusy(`${layer}:${a}`);
    api
      .neuralAction(layer, a)
      .then((r) => setMsg(`${layer}: ${String(r.decision ?? r.labeled ?? r.active ?? "ok")}`))
      .catch((e: unknown) => setError(String(e)))
      .finally(() => {
        setBusy(null);
        load();
      });
  };
  if (!st) return null;
  return (
    <div className="panel card">
      <div className="row" style={{ justifyContent: "space-between" }}>
        <h3>Neurale layers</h3>
        <button
          className="btn"
          onClick={() =>
            void api
              .rerankerDataset()
              .then((r) => setMsg(`Reranker-dataset: ${r.pairs} paren → ${r.path}`))
              .catch((e: unknown) => setError(String(e)))
          }
        >
          reranker-dataset
        </button>
      </div>
      <p style={{ color: "var(--vx-muted)", fontSize: 13 }}>
        Elk net draait eerst in schaduw naast zijn heuristiek en wordt alleen actief als het op de testset
        wint; verliest een actieve versie later, dan gaat hij automatisch terug. Training gebeurt vanzelf in
        idle (min. {st.min_examples} gelabelde voorbeelden).
      </p>
      {msg && <div className="pill ok">{msg}</div>}
      <table className="table">
        <thead>
          <tr>
            <th>Layer</th>
            <th>Modus</th>
            <th>Actief / schaduw</th>
            <th>Eval net vs heuristiek</th>
            <th>Schaduw eens</th>
            <th>Voorbeelden</th>
            <th />
          </tr>
        </thead>
        <tbody>
          {st.layers.map((l) => {
            const spec = st.specs.find((s) => s.name === l.layer);
            const last = l.manifest.versions.at(-1)?.eval;
            return (
              <tr key={l.layer} title={spec?.used_in}>
                <td>
                  <strong>{l.layer}</strong>
                  <div style={{ fontSize: 12, color: "var(--vx-muted)" }}>{spec?.description}</div>
                </td>
                <td>
                  <select
                    className="btn"
                    value={l.manifest.mode}
                    onChange={(e) =>
                      void api
                        .neuralMode(l.layer, e.target.value)
                        .then(load)
                        .catch((err: unknown) => setError(String(err)))
                    }
                  >
                    <option value="off">uit (heuristiek)</option>
                    <option value="shadow">schaduw</option>
                    <option value="active">actief</option>
                  </select>
                </td>
                <td style={{ fontSize: 12 }}>
                  {l.manifest.active ?? "–"} / {l.manifest.shadow ?? "–"}
                </td>
                <td>
                  {last ? (
                    <span className={`pill ${last.net > last.heuristic ? "ok" : "err"}`}>
                      {pct(last.net)} vs {pct(last.heuristic)} (n={last.n})
                    </span>
                  ) : (
                    "–"
                  )}
                </td>
                <td>{l.shadow_n ? pct(l.shadow_agree / l.shadow_n) : "–"}</td>
                <td>{st.examples[l.layer] ?? 0}</td>
                <td className="row" style={{ gap: 4 }}>
                  {["router", "salience", "entity", "skill", "usefulness"].includes(l.layer) && (
                    <button className="btn" disabled={!!busy} onClick={() => act(l.layer, "bootstrap")}>
                      LM-labels
                    </button>
                  )}
                  <button className="btn" disabled={!!busy} onClick={() => act(l.layer, "train")}>
                    {busy === `${l.layer}:train` ? "trainen…" : "trainen"}
                  </button>
                  {l.manifest.shadow && (
                    <button className="btn" disabled={!!busy} onClick={() => act(l.layer, "promote")}>
                      promoveren
                    </button>
                  )}
                  {l.manifest.active && (
                    <button className="btn" disabled={!!busy} onClick={() => act(l.layer, "rollback")}>
                      terug
                    </button>
                  )}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
