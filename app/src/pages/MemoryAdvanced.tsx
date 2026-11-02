// Geheugen gevorderd (W12): temporele graaf + redeneren, meta-memory, dromen + tiering, media.
import { useCallback, useEffect, useState } from "react";
import { api, type Fact, type MetaReport } from "../api/client.ts";
import { setError } from "../state/store.ts";

const pct = (x: number) => `${Math.round(x * 100)}%`;

export function GraphTab() {
  const [entity, setEntity] = useState("");
  const [at, setAt] = useState("");
  const [facts, setFacts] = useState<Fact[]>([]);
  const [hist, setHist] = useState<Fact[]>([]);
  const [msg, setMsg] = useState<string | null>(null);
  const look = () => {
    if (!entity.trim()) return;
    api
      .facts(entity.trim(), at || undefined)
      .then(setFacts)
      .catch((e: unknown) => setError(String(e)));
    api
      .factHistory(entity.trim())
      .then(setHist)
      .catch(() => undefined);
  };
  return (
    <div className="panel card">
      <h3>Kennisgraaf</h3>
      <p style={{ color: "var(--vx-muted)", fontSize: 13 }}>
        Feiten met geldigheid (van/tot). Kies een datum om te zien wat toen gold. Afgeleide feiten komen uit
        regels (transitief, symmetrisch, invers, ketens; uitbreidbaar in config/rules.toml).
      </p>
      <div className="row">
        <input
          className="btn"
          style={{ flex: 1 }}
          placeholder="Entiteit (bv. Utrecht)"
          value={entity}
          onChange={(e) => setEntity(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && look()}
        />
        <input className="btn" type="date" value={at} onChange={(e) => setAt(e.target.value)} />
        <button className="btn primary" onClick={look}>
          Bekijken
        </button>
        <button
          className="btn"
          onClick={() =>
            void api
              .reason()
              .then((r) => {
                setMsg(
                  `${r.derived} feiten afgeleid, ${r.conflicts_found} nieuwe inconsistenties → conflictregister`,
                );
                look();
              })
              .catch((e: unknown) => setError(String(e)))
          }
        >
          Redeneren
        </button>
      </div>
      {msg && <p style={{ fontSize: 13 }}>{msg}</p>}
      <table className="table">
        <tbody>
          {facts.map((f) => (
            <tr key={f.id}>
              <td>{f.src}</td>
              <td>
                <strong>{f.rel}</strong>
              </td>
              <td>{f.dst}</td>
              <td style={{ fontSize: 12, color: "var(--vx-muted)" }}>
                {f.valid_from ?? "…"} → {f.valid_to ?? "nu"}
              </td>
              <td>{pct(f.confidence)}</td>
              <td>
                {f.derived && (
                  <span className="pill" title={f.rule ?? ""}>
                    afgeleid
                  </span>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      {hist.length > 0 && (
        <>
          <h4>Historie</h4>
          {hist.map((f) => (
            <div key={f.id} style={{ fontSize: 13, opacity: f.valid_to ? 0.6 : 1 }}>
              {f.valid_from ?? "…"} → {f.valid_to ?? "nu"}: {f.src} {f.rel} {f.dst}
            </div>
          ))}
        </>
      )}
    </div>
  );
}

export function MetaTab() {
  const [m, setM] = useState<MetaReport | null>(null);
  useEffect(() => {
    api
      .meta()
      .then(setM)
      .catch((e: unknown) => setError(String(e)));
  }, []);
  if (!m) return null;
  return (
    <div className="row" style={{ alignItems: "flex-start", gap: 20 }}>
      <div className="panel card" style={{ flex: 1 }}>
        <h3>Wat Vortex weet</h3>
        <table className="table">
          <tbody>
            {Object.entries(m.by_kind).map(([k, v]) => (
              <tr key={k}>
                <td>{k}</td>
                <td>{v.count}</td>
                <td>gem. zekerheid {pct(v.avg_confidence)}</td>
              </tr>
            ))}
          </tbody>
        </table>
        <p style={{ fontSize: 13 }}>
          {m.total} actief · {m.stale} verouderd · {m.cold} op de koude schijf · {m.derived_facts} afgeleide
          feiten · {m.open_conflicts} open conflicten · {m.disputed} betwist
        </p>
        <h4>Sterkst</h4>
        {m.strongest.map((e) => (
          <div key={e.name} style={{ fontSize: 13 }}>
            {e.name} — {e.facts} feiten, {pct(e.avg_confidence)}
          </div>
        ))}
      </div>
      <div className="panel card" style={{ width: 380 }}>
        <h3>Kennisgaten</h3>
        <p style={{ color: "var(--vx-muted)", fontSize: 13 }}>
          Vaak genoemd, weinig bekend — gaan naar de leeragenda.
        </p>
        {m.gaps.length === 0 && <div className="empty">Geen gaten gevonden.</div>}
        {m.gaps.map((g) => (
          <div key={g.name} style={{ fontSize: 13 }}>
            {g.name} — {g.mentions}× genoemd, {g.facts} feiten
          </div>
        ))}
      </div>
    </div>
  );
}

export function DreamTab() {
  const [runs, setRuns] = useState<Record<string, unknown>[]>([]);
  const [tier, setTier] = useState<Awaited<ReturnType<typeof api.tier>> | null>(null);
  const [busy, setBusy] = useState(false);
  const [path, setPath] = useState("");
  const [desc, setDesc] = useState("");
  const load = useCallback(() => {
    api
      .dreams()
      .then(setRuns)
      .catch(() => undefined);
    api
      .tier()
      .then(setTier)
      .catch(() => undefined);
  }, []);
  useEffect(load, [load]);
  return (
    <div className="row" style={{ alignItems: "flex-start", gap: 20 }}>
      <div className="panel card" style={{ flex: 1 }}>
        <div className="row" style={{ justifyContent: "space-between" }}>
          <h3>Dromen</h3>
          <button
            className="btn primary"
            disabled={busy}
            onClick={() => {
              setBusy(true);
              void api
                .dream()
                .catch((e: unknown) => setError(String(e)))
                .finally(() => {
                  setBusy(false);
                  load();
                });
            }}
          >
            {busy ? "dromen…" : "Nu dromen"}
          </button>
        </div>
        <p style={{ color: "var(--vx-muted)", fontSize: 13 }}>
          Gebeurt vanzelf in idle (elke ~6 uur): samenvoegen, dag/week/maand-samenvattingen, opruimen,
          reflectie, redeneren, vertrouwen bijwerken.
        </p>
        {runs.map((r, i) => (
          <div key={i} className="skill-row" style={{ fontSize: 13 }}>
            <strong>
              {String(r.at ?? "")
                .slice(0, 16)
                .replace("T", " ")}
            </strong>{" "}
            — {String(r.merged)} samengevoegd · {String(r.day_summaries)} dag / {String(r.week_summaries)}{" "}
            week / {String(r.month_summaries)} maand · {String(r.reflections)} reflecties · {String(r.pruned)}{" "}
            opgeruimd · vertrouwen ↑{String(r.confidence_up)} ↓{String(r.confidence_down)}
          </div>
        ))}
      </div>
      <div className="panel card" style={{ width: 380 }}>
        <h3>Opslag (SSD/HDD)</h3>
        {tier && (
          <p style={{ fontSize: 13 }}>
            Warm: {tier.hot} · koud: {tier.cold} in {tier.blocks} blokken (
            {(tier.cold_bytes / 1024 / 1024).toFixed(1)} MB zstd) · SSD-cache: {tier.cached_blocks}
          </p>
        )}
        <button
          className="btn"
          onClick={() =>
            void api
              .demote()
              .then(load)
              .catch((e: unknown) => setError(String(e)))
          }
        >
          ongebruikte herinneringen naar de koude schijf
        </button>
        <h3 style={{ marginTop: 20 }}>Afbeelding of bestand onthouden</h3>
        <input
          className="btn"
          style={{ width: "100%" }}
          placeholder="pad"
          value={path}
          onChange={(e) => setPath(e.target.value)}
        />
        <input
          className="btn"
          style={{ width: "100%", marginTop: 6 }}
          placeholder="wat staat erop / waar gaat het over"
          value={desc}
          onChange={(e) => setDesc(e.target.value)}
        />
        <button
          className="btn primary"
          style={{ marginTop: 6 }}
          disabled={!path || !desc}
          onClick={() =>
            void api
              .addMedia(path, desc)
              .then(() => {
                setPath("");
                setDesc("");
              })
              .catch((e: unknown) => setError(String(e)))
          }
        >
          Onthouden
        </button>
      </div>
    </div>
  );
}

export function SelfModelPanel() {
  const [sm, setSm] = useState<Awaited<ReturnType<typeof api.selfModel>> | null>(null);
  const [ab, setAb] = useState<Awaited<ReturnType<typeof api.beliefExperiment>> | null>(null);
  useEffect(() => {
    api
      .selfModel()
      .then(setSm)
      .catch(() => undefined);
  }, []);
  return (
    <div className="row" style={{ alignItems: "flex-start", gap: 20 }}>
      <div className="panel card" style={{ flex: 1 }}>
        <h3>Zelfmodel</h3>
        <p style={{ color: "var(--vx-muted)", fontSize: 13 }}>
          Sterktes en zwaktes per taaktype uit de laatste eval; zwakke taaktypen krijgen extra zorg.
        </p>
        <table className="table">
          <tbody>
            {Object.entries(sm?.by_task ?? {}).map(([k, v]) => (
              <tr key={k}>
                <td>{k}</td>
                <td>
                  <span className={`pill ${v.n < 3 ? "" : v.score >= 0.6 ? "ok" : "err"}`}>
                    {pct(v.score)}
                  </span>
                </td>
                <td style={{ fontSize: 12 }}>n={v.n}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <div className="panel card" style={{ flex: 1 }}>
        <div className="row" style={{ justifyContent: "space-between" }}>
          <h3>Experiment: kwantum-achtig vs Bayes</h3>
          <button
            className="btn"
            onClick={() =>
              void api
                .beliefExperiment()
                .then(setAb)
                .catch((e: unknown) => setError(String(e)))
            }
          >
            draaien
          </button>
        </div>
        {ab && (
          <>
            <table className="table">
              <thead>
                <tr>
                  <th>Scenario</th>
                  <th>Bayes</th>
                  <th>Kwantum</th>
                  <th>Volgorde-effect</th>
                </tr>
              </thead>
              <tbody>
                {ab.scenarios.map((s) => (
                  <tr key={s.name}>
                    <td>{s.name}</td>
                    <td>
                      {pct(s.bayes_acc)} · Brier {s.bayes_brier.toFixed(3)}
                    </td>
                    <td>
                      {pct(s.quantum_acc)} · Brier {s.quantum_brier.toFixed(3)}
                    </td>
                    <td>{s.order_effect.toFixed(3)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
            <p style={{ fontSize: 13 }}>{ab.verdict}</p>
          </>
        )}
      </div>
    </div>
  );
}
