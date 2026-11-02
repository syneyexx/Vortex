// Wereldmonitor (W14): bronnen (tempo, voorwaarden, status), events (gemeld vs bevestigd, met
// Wikidata-entiteiten), bronbias, hypotheses, voorspellingen + kalibratie, samenvattingen,
// spelers (alleen publiek), gevolgde sites en HF-datasets (mapping bevestigen → bestemming).
import { useCallback, useEffect, useState } from "react";
import {
  api,
  type HfCatalogRow,
  type HfInspect,
  type Hypothesis,
  type Player,
  type Prediction,
  type WorldBias,
  type WorldCalibration,
  type WorldEvent,
  type WorldEventDetail,
  type WorldStatus,
  type WorldSummary,
} from "../api/client.ts";
import { setError } from "../state/store.ts";

type Tab = "events" | "sources" | "model" | "summaries" | "players" | "datasets";
const pct = (x: number) => `${(x * 100).toFixed(0)}%`;
const day = (s?: string) => (s ? s.slice(0, 16).replace("T", " ") : "–");
const fail = (e: unknown) => setError(String(e));

export function WorldPage() {
  const [tab, setTab] = useState<Tab>("events");
  const [st, setSt] = useState<WorldStatus | null>(null);
  const load = useCallback(() => {
    api
      .world()
      .then(setSt)
      .catch(() => undefined);
  }, []);
  useEffect(load, [load]);
  return (
    <div className="page">
      <h1>Wereldmonitor</h1>
      <p style={{ color: "var(--vx-muted)", fontSize: 13 }}>
        {st?.enabled
          ? "Gepland ophalen aan (inhaal-modus na een pauze)"
          : "Gepland ophalen uit (config/world.toml)"}{" "}
        ·{" "}
        {st?.counts
          ? `${st.counts.items} items · ${st.counts.events} events (${st.counts.confirmed} bevestigd) · ${st.counts.open_predictions} open voorspellingen · ${st.counts.hypotheses} hypotheses`
          : "–"}
      </p>
      <div className="chat-tabs" style={{ display: "inline-flex", margin: "8px 0 16px" }}>
        {(["events", "sources", "model", "summaries", "players", "datasets"] as const).map((t) => (
          <button key={t} className={tab === t ? "active" : ""} onClick={() => setTab(t)}>
            {
              {
                events: "Events",
                sources: "Bronnen & bias",
                model: "Hypotheses & voorspellingen",
                summaries: "Samenvattingen",
                players: "Spelers",
                datasets: "HF-datasets",
              }[t]
            }
          </button>
        ))}
      </div>
      {tab === "events" && <Events />}
      {tab === "sources" && st && <Sources st={st} reload={load} />}
      {tab === "model" && <Model />}
      {tab === "summaries" && <Summaries />}
      {tab === "players" && <Players />}
      {tab === "datasets" && <Datasets />}
    </div>
  );
}

function Events() {
  const [status, setStatus] = useState("");
  const [hours, setHours] = useState(72);
  const [evs, setEvs] = useState<WorldEvent[]>([]);
  const [open, setOpen] = useState<WorldEventDetail | null>(null);
  useEffect(() => {
    api
      .worldEvents({ status: status || undefined, hours })
      .then(setEvs)
      .catch(() => undefined);
  }, [status, hours]);
  return (
    <div className="row" style={{ alignItems: "flex-start", gap: 20 }}>
      <div className="panel card" style={{ flex: 2 }}>
        <div className="row" style={{ gap: 8, marginBottom: 8 }}>
          <select value={status} onChange={(e) => setStatus(e.target.value)}>
            <option value="">alle</option>
            <option value="confirmed">bevestigd</option>
            <option value="reported">alleen gemeld</option>
          </select>
          <select value={hours} onChange={(e) => setHours(Number(e.target.value))}>
            <option value={24}>24 uur</option>
            <option value={72}>3 dagen</option>
            <option value={168}>week</option>
            <option value={720}>maand</option>
          </select>
        </div>
        <table className="table">
          <thead>
            <tr>
              <th>Event</th>
              <th>Status</th>
              <th>Bronnen</th>
              <th>Thema</th>
              <th>Regio</th>
              <th>Laatst</th>
            </tr>
          </thead>
          <tbody>
            {evs.map((e) => (
              <tr
                key={e.id}
                style={{ cursor: "pointer" }}
                onClick={() => void api.worldEvent(e.id).then(setOpen).catch(fail)}
              >
                <td>{e.title}</td>
                <td>
                  <span className={`pill ${e.status === "confirmed" ? "ok" : ""}`}>
                    {e.status === "confirmed" ? "bevestigd" : "gemeld"}
                  </span>
                </td>
                <td>{e.n_items}</td>
                <td>{e.theme}</td>
                <td>{e.region}</td>
                <td>{day(e.last_seen)}</td>
              </tr>
            ))}
          </tbody>
        </table>
        {evs.length === 0 && <p style={{ color: "var(--vx-muted)" }}>Nog geen events in dit venster.</p>}
      </div>
      {open && (
        <div className="panel card" style={{ flex: 1 }}>
          <h3>{open.title}</h3>
          <p style={{ fontSize: 13 }}>
            {open.status === "confirmed" ? "Bevestigd" : "Alleen gemeld"} door {open.groups.length}{" "}
            onafhankelijke bron(nen) · {day(open.first_seen)} → {day(open.last_seen)}
            {open.memory_id && " · in geheugen (KG)"}
          </p>
          {open.entities?.some((x) => x.qid) && (
            <p style={{ fontSize: 13 }}>
              {open.entities
                .filter((x) => x.qid)
                .map((x) => (
                  <a
                    key={x.qid}
                    href={`https://www.wikidata.org/wiki/${x.qid}`}
                    target="_blank"
                    rel="noreferrer"
                    style={{ marginRight: 8 }}
                  >
                    {x.label} ({x.qid})
                  </a>
                ))}
            </p>
          )}
          <ul style={{ fontSize: 13 }}>
            {open.items.map((i, k) => (
              <li key={k}>
                <b>{i.source}</b> {i.country && `(${i.country})`}:{" "}
                {i.url ? (
                  <a href={i.url} target="_blank" rel="noreferrer">
                    {i.title}
                  </a>
                ) : (
                  i.title
                )}{" "}
                <span style={{ color: "var(--vx-muted)" }}>{day(i.published)}</span>
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}

function Sources({ st, reload }: { st: WorldStatus; reload: () => void }) {
  const [bias, setBias] = useState<WorldBias[]>([]);
  const [busy, setBusy] = useState<string | null>(null);
  const [url, setUrl] = useState("");
  const [iv, setIv] = useState(24);
  useEffect(() => {
    api
      .worldBias()
      .then(setBias)
      .catch(() => undefined);
  }, [st]);
  const fetchNow = (s?: string) => {
    setBusy(s ?? "all");
    api
      .worldFetch(s)
      .catch(fail)
      .finally(() => {
        setBusy(null);
        reload();
      });
  };
  return (
    <>
      <div className="panel card">
        <div className="row" style={{ justifyContent: "space-between" }}>
          <h3>Bronnen</h3>
          <button className="btn" disabled={!!busy} onClick={() => fetchNow()}>
            {busy === "all" ? "bezig…" : "Alles nu ophalen"}
          </button>
        </div>
        <table className="table">
          <thead>
            <tr>
              <th>Bron</th>
              <th>Soort</th>
              <th>Land / richting</th>
              <th>Interval</th>
              <th>Laatst gelukt</th>
              <th>Items</th>
              <th>Fouten</th>
              <th>Voorwaarden</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {st.sources.map((s) => (
              <tr key={s.name} style={{ opacity: s.enabled ? 1 : 0.5 }}>
                <td>{s.name}</td>
                <td>{s.kind}</td>
                <td>
                  {s.country ?? "–"} {s.orientation && `· ${s.orientation}`}
                </td>
                <td>{s.interval_min} min</td>
                <td>{day(s.stats.last_ok)}</td>
                <td>{s.stats.items ?? 0}</td>
                <td title={s.stats.last_error ?? ""}>{s.stats.errors ?? 0}</td>
                <td style={{ fontSize: 12, maxWidth: 280 }}>
                  {s.terms} (≥ {s.min_gap_s} s tussen verzoeken)
                </td>
                <td>
                  <button className="btn" disabled={!!busy || !s.enabled} onClick={() => fetchNow(s.name)}>
                    {busy === s.name ? "…" : "nu"}
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <div className="row" style={{ alignItems: "flex-start", gap: 20, marginTop: 16 }}>
        <div className="panel card" style={{ flex: 1 }}>
          <h3>Bronbias</h3>
          <table className="table">
            <thead>
              <tr>
                <th>Bron</th>
                <th>Richting</th>
                <th>Items</th>
                <th>Aandeel in bevestigde events</th>
                <th>Landen gedekt</th>
              </tr>
            </thead>
            <tbody>
              {bias.map((b) => (
                <tr key={b.source}>
                  <td>{b.source}</td>
                  <td>
                    {b.home ?? ""} {b.orientation ?? ""}
                  </td>
                  <td>{b.items}</td>
                  <td>{pct(b.confirmed_share)}</td>
                  <td>{b.regions}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        <div className="panel card" style={{ flex: 1 }}>
          <h3>Gevolgde sites (recrawl-schema)</h3>
          <div className="row" style={{ gap: 6 }}>
            <input
              placeholder="https://…"
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              style={{ flex: 1 }}
            />
            <input
              type="number"
              min={0.1}
              step={1}
              value={iv}
              onChange={(e) => setIv(Number(e.target.value))}
              style={{ width: 70 }}
            />
            <span>uur</span>
            <button
              className="btn"
              disabled={!url}
              onClick={() =>
                void api
                  .follow(url, iv)
                  .then(() => {
                    setUrl("");
                    reload();
                  })
                  .catch(fail)
              }
            >
              Volgen
            </button>
          </div>
          <ul style={{ fontSize: 13 }}>
            {st.followed.map((f) => (
              <li key={f.url}>
                {f.url} · elke {f.interval_hours} u · {f.changes} wijzigingen · laatst {day(f.last_fetch)}{" "}
                <button
                  className="btn"
                  onClick={() => void api.follow(f.url, 0, true).then(reload).catch(fail)}
                >
                  ×
                </button>
              </li>
            ))}
          </ul>
        </div>
      </div>
    </>
  );
}

function Model() {
  const [hyps, setHyps] = useState<Hypothesis[]>([]);
  const [preds, setPreds] = useState<Prediction[]>([]);
  const [cal, setCal] = useState<WorldCalibration | null>(null);
  const [form, setForm] = useState({ claim: "", probability: 0.6, criteria: "", resolve_by: "" });
  const load = useCallback(() => {
    api
      .hypotheses()
      .then(setHyps)
      .catch(() => undefined);
    api
      .predictions()
      .then(setPreds)
      .catch(() => undefined);
    api
      .worldCalibration()
      .then(setCal)
      .catch(() => undefined);
  }, []);
  useEffect(load, [load]);
  return (
    <>
      <div className="row" style={{ alignItems: "flex-start", gap: 20 }}>
        <div className="panel card" style={{ flex: 1 }}>
          <h3>Kalibratie wereldmodel</h3>
          {cal && (
            <p style={{ fontSize: 13 }}>
              {cal.overall.n} beslist · Brier {cal.overall.brier.toFixed(3)} · ECE{" "}
              {cal.overall.ece.toFixed(3)} · nauwkeurigheid {pct(cal.overall.accuracy)}
              <br />
              {Object.entries(cal.counts)
                .map(([k, v]) => `${k}: ${v}`)
                .join(" · ")}
            </p>
          )}
          {cal &&
            Object.entries(cal.by).map(([k, v]) => (
              <div key={k} style={{ fontSize: 12 }}>
                {k}: n={v.n}, Brier {v.brier.toFixed(3)}
              </div>
            ))}
          <button className="btn" onClick={() => void api.resolvePredictions().then(load).catch(fail)}>
            Verlopen voorspellingen nu toetsen
          </button>
        </div>
        <div className="panel card" style={{ flex: 1 }}>
          <h3>Nieuwe voorspelling</h3>
          <input
            placeholder="Claim"
            value={form.claim}
            onChange={(e) => setForm({ ...form, claim: e.target.value })}
            style={{ width: "100%" }}
          />
          <input
            placeholder="Toetscriteria (wat telt als waar)"
            value={form.criteria}
            onChange={(e) => setForm({ ...form, criteria: e.target.value })}
            style={{ width: "100%", marginTop: 6 }}
          />
          <div className="row" style={{ gap: 6, marginTop: 6 }}>
            <input
              type="number"
              min={0.01}
              max={0.99}
              step={0.05}
              value={form.probability}
              onChange={(e) => setForm({ ...form, probability: Number(e.target.value) })}
              style={{ width: 80 }}
            />
            <input
              type="date"
              value={form.resolve_by}
              onChange={(e) => setForm({ ...form, resolve_by: e.target.value })}
            />
            <button
              className="btn"
              disabled={!form.claim || !form.criteria || !form.resolve_by}
              onClick={() => void api.addPrediction(form).then(load).catch(fail)}
            >
              Vastleggen
            </button>
          </div>
        </div>
      </div>
      <div className="panel card" style={{ marginTop: 16 }}>
        <h3>Voorspellingen</h3>
        <table className="table">
          <thead>
            <tr>
              <th>Claim</th>
              <th>Kans</th>
              <th>Deadline</th>
              <th>Herkomst</th>
              <th>Uitkomst</th>
            </tr>
          </thead>
          <tbody>
            {preds.map((p) => (
              <tr key={p.id} title={p.criteria}>
                <td>{p.claim}</td>
                <td>{pct(p.probability)}</td>
                <td>{p.resolve_by.slice(0, 10)}</td>
                <td>{p.actor ?? p.origin}</td>
                <td>
                  <span className={`pill ${p.status === "true" ? "ok" : ""}`}>
                    {
                      { open: "open", true: "waar", false: "onwaar", unresolvable: "niet te beslissen" }[
                        p.status
                      ]
                    }
                  </span>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <div className="panel card" style={{ marginTop: 16 }}>
        <h3>Causale hypotheses</h3>
        <table className="table">
          <thead>
            <tr>
              <th>Oorzaak</th>
              <th>Gevolg</th>
              <th>Vertrouwen</th>
              <th>Steun / contra</th>
            </tr>
          </thead>
          <tbody>
            {hyps.map((h) => (
              <tr key={h.id}>
                <td>{h.cause}</td>
                <td>{h.effect}</td>
                <td>{pct(h.confidence)}</td>
                <td>
                  {h.support} / {h.contra}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </>
  );
}

function Summaries() {
  const [period, setPeriod] = useState("day");
  const [groupBy, setGroupBy] = useState("theme");
  const [rows, setRows] = useState<WorldSummary[]>([]);
  const [busy, setBusy] = useState(false);
  return (
    <div className="panel card">
      <div className="row" style={{ gap: 8 }}>
        <select value={period} onChange={(e) => setPeriod(e.target.value)}>
          <option value="day">dag</option>
          <option value="week">week</option>
          <option value="month">maand</option>
        </select>
        <select value={groupBy} onChange={(e) => setGroupBy(e.target.value)}>
          <option value="theme">per thema</option>
          <option value="region">per regio</option>
        </select>
        <button
          className="btn"
          disabled={busy}
          onClick={() => {
            setBusy(true);
            api
              .worldSummaries(period, groupBy)
              .then(setRows)
              .catch(fail)
              .finally(() => setBusy(false));
          }}
        >
          {busy ? "bezig…" : "Samenvatten"}
        </button>
      </div>
      {rows.map((r) => (
        <div key={r.key} style={{ marginTop: 12 }}>
          <h4>
            {r.key}{" "}
            <span style={{ color: "var(--vx-muted)", fontSize: 12 }}>
              ({r.events} events, {r.confirmed} bevestigd)
            </span>
          </h4>
          <p style={{ fontSize: 14 }}>{r.summary}</p>
        </div>
      ))}
    </div>
  );
}

function Players() {
  const [ps, setPs] = useState<Player[]>([]);
  const [form, setForm] = useState({ name: "", sector: "", kind: "organization", public_role: "" });
  const [out, setOut] = useState<string | null>(null);
  const [sit, setSit] = useState("");
  const load = useCallback(() => {
    api
      .players()
      .then(setPs)
      .catch(() => undefined);
  }, []);
  useEffect(load, [load]);
  return (
    <div className="row" style={{ alignItems: "flex-start", gap: 20 }}>
      <div className="panel card" style={{ flex: 1 }}>
        <h3>Speler toevoegen</h3>
        <p style={{ fontSize: 12, color: "var(--vx-muted)" }}>
          Alleen organisaties en publieke figuren in hun publieke rol (bv. &quot;CEO van …&quot;) — geen
          privépersonen.
        </p>
        <input
          placeholder="Naam"
          value={form.name}
          onChange={(e) => setForm({ ...form, name: e.target.value })}
          style={{ width: "100%" }}
        />
        <input
          placeholder="Sector"
          value={form.sector}
          onChange={(e) => setForm({ ...form, sector: e.target.value })}
          style={{ width: "100%", marginTop: 6 }}
        />
        <div className="row" style={{ gap: 6, marginTop: 6 }}>
          <select value={form.kind} onChange={(e) => setForm({ ...form, kind: e.target.value })}>
            <option value="organization">organisatie</option>
            <option value="person">publiek figuur</option>
          </select>
          <input
            placeholder="Publieke rol"
            value={form.public_role}
            onChange={(e) => setForm({ ...form, public_role: e.target.value })}
            style={{ flex: 1 }}
          />
        </div>
        <button
          className="btn"
          style={{ marginTop: 6 }}
          disabled={!form.name || !form.sector}
          onClick={() => void api.addPlayer(form).then(load).catch(fail)}
        >
          Toevoegen
        </button>
        {out && <pre style={{ fontSize: 12, whiteSpace: "pre-wrap" }}>{out}</pre>}
      </div>
      <div className="panel card" style={{ flex: 2 }}>
        <h3>Spelers per sector</h3>
        <input
          placeholder='Situatie voor "wat zou X doen?"'
          value={sit}
          onChange={(e) => setSit(e.target.value)}
          style={{ width: "100%", marginBottom: 8 }}
        />
        <table className="table">
          <thead>
            <tr>
              <th>Naam</th>
              <th>Sector</th>
              <th>Soort</th>
              <th>Bronnen</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {ps.map((p) => (
              <tr key={p.id}>
                <td>{p.name}</td>
                <td>{p.sector}</td>
                <td>{p.kind === "person" ? `publiek figuur · ${p.notes}` : "organisatie"}</td>
                <td>{p.sources.length}</td>
                <td>
                  <button
                    className="btn"
                    onClick={() =>
                      void api
                        .studyPlayer(p.id)
                        .then((r) => {
                          setOut(`${p.name}: ${r.length} beslissingen/principes/mislukkingen geëxtraheerd`);
                          load();
                        })
                        .catch(fail)
                    }
                  >
                    Bestuderen
                  </button>{" "}
                  <button
                    className="btn"
                    disabled={!sit}
                    onClick={() =>
                      void api
                        .predictPlayer(p.id, sit, 30)
                        .then((r) =>
                          setOut(
                            `Voorspelling (${pct(r.probability)}, toetsen op ${r.resolve_by.slice(0, 10)}): ${r.claim}`,
                          ),
                        )
                        .catch(fail)
                    }
                  >
                    Wat zou {p.name} doen?
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

function Datasets() {
  const [source, setSource] = useState("");
  const [license, setLicense] = useState("");
  const [info, setInfo] = useState<HfInspect | null>(null);
  const [mapping, setMapping] = useState<Record<string, string>>({});
  const [dest, setDest] = useState("knowledge");
  const [cat, setCat] = useState<HfCatalogRow[]>([]);
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<string | null>(null);
  const load = useCallback(() => {
    api
      .hfCatalog()
      .then(setCat)
      .catch(() => undefined);
  }, []);
  useEffect(load, [load]);
  const inspect = () => {
    setBusy(true);
    api
      .hfInspect(source, license)
      .then((i) => {
        setInfo(i);
        setMapping(i.lm_mapping ?? i.mapping);
        setDest(i.destinations[0] ?? "knowledge");
      })
      .catch(fail)
      .finally(() => setBusy(false));
  };
  const doImport = () => {
    setBusy(true);
    api
      .hfImport({
        source,
        destination: dest,
        mapping,
        kind: info?.lm_kind ?? info?.schema.kind,
        license: license || undefined,
      })
      .then((r) => {
        setMsg(`Geïmporteerd: ${JSON.stringify(r.stats)}`);
        load();
      })
      .catch(fail)
      .finally(() => setBusy(false));
  };
  const fields = info ? Object.keys(info.schema.fields) : [];
  return (
    <>
      <div className="panel card">
        <h3>Dataset importeren</h3>
        <div className="row" style={{ gap: 6 }}>
          <input
            placeholder="HF-ID (bv. openai/gsm8k) of lokaal pad"
            value={source}
            onChange={(e) => setSource(e.target.value)}
            style={{ flex: 1 }}
          />
          <input
            placeholder="licentie (als niet in de card)"
            value={license}
            onChange={(e) => setLicense(e.target.value)}
          />
          <button className="btn" disabled={!source || busy} onClick={inspect}>
            {busy && !info ? "bezig…" : "Inspecteren"}
          </button>
        </div>
        {info && (
          <div style={{ marginTop: 12 }}>
            <p style={{ fontSize: 13 }}>
              Schema: <b>{info.lm_kind ?? info.schema.kind}</b> · licentie:{" "}
              <b>{info.license ?? "onbekend"}</b>
              {info.lm_why && <> · LM: {info.lm_why}</>}
            </p>
            <p style={{ fontSize: 12, color: "var(--vx-muted)" }}>
              Controleer en bevestig de mapping (doelveld ← bronveld):
            </p>
            {["prompt", "response", "messages", "chosen", "rejected", "text", "title", "label", "code"].map(
              (t) => (
                <div key={t} className="row" style={{ gap: 6, fontSize: 13 }}>
                  <span style={{ width: 90 }}>{t}</span>
                  <select
                    value={mapping[t] ?? ""}
                    onChange={(e) => {
                      const v = e.target.value;
                      setMapping(
                        Object.fromEntries(
                          Object.entries({ ...mapping, [t]: v }).filter(([, x]) => x !== ""),
                        ),
                      );
                    }}
                  >
                    <option value="">—</option>
                    {fields.map((f) => (
                      <option key={f} value={f}>
                        {f}
                      </option>
                    ))}
                  </select>
                </div>
              ),
            )}
            <details style={{ marginTop: 8 }}>
              <summary>Voorbeeldrijen</summary>
              <pre style={{ fontSize: 11, whiteSpace: "pre-wrap" }}>
                {JSON.stringify(info.sample, null, 2)}
              </pre>
            </details>
            <div className="row" style={{ gap: 6, marginTop: 8 }}>
              <select value={dest} onChange={(e) => setDest(e.target.value)}>
                {["knowledge", "sft", "dpo", "layer", "eval"].map((d) => (
                  <option key={d} value={d} disabled={!info.destinations.includes(d)}>
                    {
                      {
                        knowledge: "kennis (geheugen)",
                        sft: "LM-training SFT",
                        dpo: "LM-training DPO",
                        layer: "neurale layer",
                        eval: "eval-set",
                      }[d]
                    }
                  </option>
                ))}
              </select>
              <button className="btn" disabled={busy || Object.keys(mapping).length === 0} onClick={doImport}>
                {busy ? "bezig…" : "Bevestigen & importeren"}
              </button>
            </div>
          </div>
        )}
        {msg && <p style={{ fontSize: 12 }}>{msg}</p>}
      </div>
      <div className="panel card" style={{ marginTop: 16 }}>
        <h3>Catalogus</h3>
        <table className="table">
          <thead>
            <tr>
              <th>Bron</th>
              <th>Schema</th>
              <th>Bestemming</th>
              <th>Licentie</th>
              <th>Rijen → bewaard</th>
              <th>Dedup</th>
              <th>Decontam.</th>
              <th>Versie</th>
            </tr>
          </thead>
          <tbody>
            {cat.map((c) => (
              <tr key={c.id}>
                <td>{c.source}</td>
                <td>{c.schema}</td>
                <td>{c.destination}</td>
                <td>{c.license ?? "?"}</td>
                <td>
                  {c.rows} → {c.kept}
                </td>
                <td>{c.deduped}</td>
                <td>{c.decontaminated}</td>
                <td>
                  <code>{c.version}</code>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </>
  );
}
