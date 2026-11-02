// Autonomie (W15): overzicht (briefing, suggesties, lopend werk), taken & projecten, triggers,
// voortgang ("hoe slim is Vortex"), systeem (GPU-bewaker, backups, API en telefoon).
import { useCallback, useEffect, useState } from "react";
import {
  API_BASE,
  api,
  type AutonomyOverview,
  type VxBackup,
  type VxGpu,
  type VxProgress,
  type VxProject,
  type VxSuggestion,
  type VxTask,
  type VxTrigger,
} from "../api/client.ts";
import { setError } from "../state/store.ts";

type Tab = "overview" | "tasks" | "triggers" | "progress" | "system";
const fail = (e: unknown) => setError(String(e));
const t16 = (s?: string | null) => (s ? s.slice(0, 16).replace("T", " ") : "–");
const pct = (x: number) => `${(x * 100).toFixed(0)}%`;
const STATUS: Record<string, string> = {
  queued: "wachtrij",
  running: "bezig",
  done: "klaar",
  failed: "mislukt",
  cancelled: "geannuleerd",
  paused: "gepauzeerd",
};
const KINDS = [
  "prompt",
  "research",
  "agenda",
  "world",
  "dream",
  "eval",
  "train_layer",
  "ingest",
  "briefing",
  "backup",
  "monitor",
];

export function AutonomyPage() {
  const [tab, setTab] = useState<Tab>("overview");
  const [ov, setOv] = useState<AutonomyOverview | null>(null);
  useEffect(() => {
    const load = () =>
      void api
        .autonomy()
        .then(setOv)
        .catch(() => undefined);
    load();
    const t = setInterval(load, 5000);
    return () => clearInterval(t);
  }, []);
  return (
    <div className="page">
      <h1>Autonomie</h1>
      <p style={{ color: "var(--vx-muted)", fontSize: 13 }}>
        {ov
          ? `Gebruiker ${ov.user_idle ? "niet actief (idle-werk mag)" : "actief"}${ov.external_gpu_load ? " · ander programma gebruikt de GPU" : ""} · ${ov.running.length} bezig · ${ov.queued} in de wachtrij · ${ov.suggestions} suggestie(s)`
          : "–"}
      </p>
      <div className="chat-tabs" style={{ display: "inline-flex", margin: "8px 0 16px" }}>
        {(["overview", "tasks", "triggers", "progress", "system"] as const).map((t) => (
          <button key={t} className={tab === t ? "active" : ""} onClick={() => setTab(t)}>
            {
              {
                overview: "Overzicht",
                tasks: "Taken & projecten",
                triggers: "Planning & triggers",
                progress: "Hoe slim is Vortex",
                system: "Systeem",
              }[t]
            }
          </button>
        ))}
      </div>
      {tab === "overview" && <Overview />}
      {tab === "tasks" && <Tasks />}
      {tab === "triggers" && <Triggers />}
      {tab === "progress" && <Progress />}
      {tab === "system" && <System ov={ov} />}
    </div>
  );
}

function Overview() {
  const [brief, setBrief] = useState<{ day: string; markdown: string }[]>([]);
  const [sug, setSug] = useState<VxSuggestion[]>([]);
  const [busy, setBusy] = useState(false);
  const load = useCallback(() => {
    api
      .briefings()
      .then(setBrief)
      .catch(() => undefined);
    api
      .suggestions("new")
      .then(setSug)
      .catch(() => undefined);
  }, []);
  useEffect(load, [load]);
  const b = brief[0];
  return (
    <div className="row" style={{ alignItems: "flex-start", gap: 20 }}>
      <div className="panel card" style={{ flex: 2 }}>
        <div className="row" style={{ justifyContent: "space-between" }}>
          <h3>Briefing {b?.day ?? ""}</h3>
          <button
            className="btn"
            disabled={busy}
            onClick={() => {
              setBusy(true);
              api
                .briefingNow()
                .then(load)
                .catch(fail)
                .finally(() => setBusy(false));
            }}
          >
            {busy ? "bezig…" : "Nu schrijven"}
          </button>
        </div>
        {b ? (
          <pre style={{ whiteSpace: "pre-wrap", fontFamily: "inherit", fontSize: 14 }}>{b.markdown}</pre>
        ) : (
          <p style={{ color: "var(--vx-muted)" }}>Nog geen briefing (elke dag om 07:30, of nu schrijven).</p>
        )}
        {brief.length > 1 && (
          <details>
            <summary>Eerdere briefings</summary>
            {brief.slice(1).map((x) => (
              <details key={x.day} style={{ marginLeft: 12 }}>
                <summary>{x.day}</summary>
                <pre style={{ whiteSpace: "pre-wrap", fontFamily: "inherit", fontSize: 13 }}>
                  {x.markdown}
                </pre>
              </details>
            ))}
          </details>
        )}
      </div>
      <div className="panel card" style={{ flex: 1 }}>
        <div className="row" style={{ justifyContent: "space-between" }}>
          <h3>Suggesties</h3>
          <button className="btn" onClick={() => void api.scanSuggestions().then(load).catch(fail)}>
            Nu kijken
          </button>
        </div>
        {sug.length === 0 && <p style={{ color: "var(--vx-muted)" }}>Niets te melden.</p>}
        {sug.map((s) => (
          <div key={s.id} style={{ marginBottom: 10, fontSize: 14 }}>
            {s.text}
            <div className="row" style={{ gap: 6, marginTop: 4 }}>
              {s.action && (
                <button
                  className="btn"
                  onClick={() => void api.decideSuggestion(s.id, true).then(load).catch(fail)}
                >
                  Doen: {s.action.title}
                </button>
              )}
              <button
                className="btn"
                onClick={() => void api.decideSuggestion(s.id, false).then(load).catch(fail)}
              >
                Negeren
              </button>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

function TaskTable({ tasks, reload }: { tasks: VxTask[]; reload: () => void }) {
  const act = (id: string, a: "cancel" | "pause" | "resume") =>
    void api.taskAction(id, a).then(reload).catch(fail);
  return (
    <table className="table">
      <thead>
        <tr>
          <th>Taak</th>
          <th>Soort</th>
          <th>Prio</th>
          <th>Status</th>
          <th>Pogingen</th>
          <th>Bijgewerkt</th>
          <th />
        </tr>
      </thead>
      <tbody>
        {tasks.map((t) => (
          <tr key={t.id} title={t.error ?? (t.result ? JSON.stringify(t.result).slice(0, 400) : "")}>
            <td>
              {t.title}
              {t.depends_on.length > 0 && (
                <span style={{ color: "var(--vx-muted)", fontSize: 12 }}>
                  {" "}
                  · wacht op {t.depends_on.length}
                </span>
              )}
              {t.needs_idle && <span style={{ color: "var(--vx-muted)", fontSize: 12 }}> · idle</span>}
            </td>
            <td>{t.kind}</td>
            <td>P{t.priority}</td>
            <td>
              <span className={`pill ${t.status === "done" ? "ok" : ""}`}>
                {STATUS[t.status] ?? t.status}
              </span>
              {t.error && (
                <div style={{ fontSize: 11, color: "var(--vx-muted)" }}>{t.error.slice(0, 80)}</div>
              )}
            </td>
            <td>
              {t.attempts}/{t.max_attempts}
            </td>
            <td>{t16(t.updated_at)}</td>
            <td>
              {(t.status === "queued" || t.status === "running") && (
                <button className="btn" onClick={() => act(t.id, "pause")}>
                  pauze
                </button>
              )}
              {(t.status === "paused" || t.status === "failed") && (
                <button className="btn" onClick={() => act(t.id, "resume")}>
                  hervat
                </button>
              )}{" "}
              {["queued", "running", "paused"].includes(t.status) && (
                <button className="btn" onClick={() => act(t.id, "cancel")}>
                  ×
                </button>
              )}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function Tasks() {
  const [tasks, setTasks] = useState<VxTask[]>([]);
  const [projects, setProjects] = useState<VxProject[]>([]);
  const [form, setForm] = useState({
    title: "",
    kind: "prompt",
    text: "",
    priority: 2,
    project_id: "",
    idle: false,
    after: "",
  });
  const [pf, setPf] = useState({ name: "", goal: "" });
  const [note, setNote] = useState<Record<string, string>>({});
  const load = useCallback(() => {
    api
      .tasks()
      .then(setTasks)
      .catch(() => undefined);
    api
      .projects()
      .then(setProjects)
      .catch(() => undefined);
  }, []);
  useEffect(() => {
    load();
    const t = setInterval(load, 4000);
    return () => clearInterval(t);
  }, [load]);
  const params = (): Record<string, unknown> => {
    if (form.kind === "prompt") return { prompt: form.text };
    if (form.kind === "research") return { question: form.text, mode: "standard" };
    if (form.kind === "ingest") return { paths: form.text.split("\n").filter(Boolean) };
    if (form.kind === "train_layer") return { layer: form.text };
    if (form.kind === "eval") return form.text ? { suite: form.text } : {};
    return {};
  };
  return (
    <>
      <div className="row" style={{ alignItems: "flex-start", gap: 20 }}>
        <div className="panel card" style={{ flex: 1 }}>
          <h3>Nieuwe taak</h3>
          <input
            placeholder="Titel"
            value={form.title}
            onChange={(e) => setForm({ ...form, title: e.target.value })}
            style={{ width: "100%" }}
          />
          <div className="row" style={{ gap: 6, marginTop: 6 }}>
            <select value={form.kind} onChange={(e) => setForm({ ...form, kind: e.target.value })}>
              {KINDS.map((k) => (
                <option key={k}>{k}</option>
              ))}
            </select>
            <select
              value={form.priority}
              onChange={(e) => setForm({ ...form, priority: Number(e.target.value) })}
            >
              <option value={1}>P1 hoog</option>
              <option value={2}>P2</option>
              <option value={3}>P3 achtergrond</option>
            </select>
            <select
              value={form.project_id}
              onChange={(e) => setForm({ ...form, project_id: e.target.value })}
            >
              <option value="">geen project</option>
              {projects.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name}
                </option>
              ))}
            </select>
          </div>
          <textarea
            placeholder="Vraag / onderzoeksvraag / paden / layer / suite"
            value={form.text}
            onChange={(e) => setForm({ ...form, text: e.target.value })}
            style={{ width: "100%", marginTop: 6, minHeight: 60 }}
          />
          <div className="row" style={{ gap: 6, marginTop: 6, fontSize: 13 }}>
            <label>
              <input
                type="checkbox"
                checked={form.idle}
                onChange={(e) => setForm({ ...form, idle: e.target.checked })}
              />{" "}
              alleen als ik niet actief ben
            </label>
            <select value={form.after} onChange={(e) => setForm({ ...form, after: e.target.value })}>
              <option value="">direct</option>
              {tasks
                .filter((t) => ["queued", "running"].includes(t.status))
                .map((t) => (
                  <option key={t.id} value={t.id}>
                    na: {t.title}
                  </option>
                ))}
            </select>
            <button
              className="btn"
              disabled={!form.title}
              onClick={() =>
                void api
                  .addTask({
                    title: form.title,
                    kind: form.kind,
                    params: params(),
                    priority: form.priority,
                    needs_idle: form.idle,
                    project_id: form.project_id || undefined,
                    depends_on: form.after ? [form.after] : [],
                  })
                  .then(() => {
                    setForm({ ...form, title: "", text: "" });
                    load();
                  })
                  .catch(fail)
              }
            >
              In de wachtrij
            </button>
          </div>
        </div>
        <div className="panel card" style={{ flex: 1 }}>
          <h3>Projecten (lopen over sessies heen)</h3>
          <div className="row" style={{ gap: 6 }}>
            <input
              placeholder="Naam"
              value={pf.name}
              onChange={(e) => setPf({ ...pf, name: e.target.value })}
            />
            <input
              placeholder="Doel"
              value={pf.goal}
              onChange={(e) => setPf({ ...pf, goal: e.target.value })}
              style={{ flex: 1 }}
            />
            <button
              className="btn"
              disabled={!pf.name}
              onClick={() =>
                void api
                  .addProject(pf.name, pf.goal)
                  .then(() => {
                    setPf({ name: "", goal: "" });
                    load();
                  })
                  .catch(fail)
              }
            >
              +
            </button>
          </div>
          {projects.map((p) => (
            <details key={p.id} style={{ marginTop: 8 }}>
              <summary>
                <b>{p.name}</b> · {pct(p.progress)} · {p.tasks.length} taken · {p.status}
              </summary>
              <p style={{ fontSize: 13 }}>{p.goal}</p>
              <pre style={{ whiteSpace: "pre-wrap", fontSize: 12 }}>{p.notes || "(nog geen notities)"}</pre>
              <div className="row" style={{ gap: 6 }}>
                <input
                  placeholder="Notitie / volgende stap"
                  value={note[p.id] ?? ""}
                  onChange={(e) => setNote({ ...note, [p.id]: e.target.value })}
                  style={{ flex: 1 }}
                />
                <button
                  className="btn"
                  onClick={() =>
                    void api
                      .updateProject(p.id, { note: note[p.id] })
                      .then(() => {
                        setNote({ ...note, [p.id]: "" });
                        load();
                      })
                      .catch(fail)
                  }
                >
                  Noteren
                </button>
                <select
                  value={p.status}
                  onChange={(e) =>
                    void api.updateProject(p.id, { status: e.target.value }).then(load).catch(fail)
                  }
                >
                  <option value="active">actief</option>
                  <option value="paused">gepauzeerd</option>
                  <option value="done">klaar</option>
                </select>
              </div>
            </details>
          ))}
        </div>
      </div>
      <div className="panel card" style={{ marginTop: 16 }}>
        <h3>Taakqueue</h3>
        <TaskTable tasks={tasks} reload={load} />
      </div>
    </>
  );
}

function Triggers() {
  const [trg, setTrg] = useState<VxTrigger[]>([]);
  const [f, setF] = useState({
    name: "",
    kind: "cron",
    spec: "0 9 * * 1-5",
    title: "",
    task: "prompt",
    text: "",
  });
  const load = useCallback(() => {
    api
      .triggers()
      .then(setTrg)
      .catch(() => undefined);
  }, []);
  useEffect(load, [load]);
  return (
    <>
      <div className="panel card">
        <h3>Nieuwe trigger</h3>
        <div className="row" style={{ gap: 6, flexWrap: "wrap" }}>
          <input placeholder="Naam" value={f.name} onChange={(e) => setF({ ...f, name: e.target.value })} />
          <select
            value={f.kind}
            onChange={(e) =>
              setF({
                ...f,
                kind: e.target.value,
                spec:
                  { cron: "0 9 * * 1-5", interval: "60", event: "world.event.updated|status=confirmed" }[
                    e.target.value
                  ] ?? "",
              })
            }
          >
            <option value="cron">tijd (cron)</option>
            <option value="interval">interval (min)</option>
            <option value="event">gebeurtenis</option>
          </select>
          <input
            value={f.spec}
            onChange={(e) => setF({ ...f, spec: e.target.value })}
            style={{ width: 240 }}
            title="cron: minuut uur dag maand weekdag · event: soort-patroon|veld=waarde"
          />
          <select value={f.task} onChange={(e) => setF({ ...f, task: e.target.value })}>
            {KINDS.map((k) => (
              <option key={k}>{k}</option>
            ))}
          </select>
          <input
            placeholder="Taaktitel"
            value={f.title}
            onChange={(e) => setF({ ...f, title: e.target.value })}
          />
          <input
            placeholder="Vraag (prompt/research)"
            value={f.text}
            onChange={(e) => setF({ ...f, text: e.target.value })}
            style={{ flex: 1 }}
          />
          <button
            className="btn"
            disabled={!f.name || !f.title}
            onClick={() =>
              void api
                .addTrigger({
                  name: f.name,
                  kind: f.kind,
                  spec: f.spec,
                  task: {
                    title: f.title,
                    kind: f.task,
                    params: f.task === "research" ? { question: f.text } : { prompt: f.text },
                    priority: 2,
                    needs_idle: false,
                  },
                })
                .then(load)
                .catch(fail)
            }
          >
            Toevoegen
          </button>
        </div>
      </div>
      <div className="panel card" style={{ marginTop: 16 }}>
        <table className="table">
          <thead>
            <tr>
              <th>Trigger</th>
              <th>Wanneer</th>
              <th>Taak</th>
              <th>Laatst</th>
              <th>Volgende</th>
              <th>Keer</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {trg.map((t) => (
              <tr key={t.id} style={{ opacity: t.enabled ? 1 : 0.5 }}>
                <td>
                  {t.name} {t.builtin && <span className="pill">standaard</span>}
                </td>
                <td>
                  <code>
                    {t.kind}: {t.spec}
                  </code>
                </td>
                <td>
                  {t.task.title} ({t.task.kind})
                </td>
                <td>{t16(t.last_fired)}</td>
                <td>{t16(t.next)}</td>
                <td>{t.fired}</td>
                <td>
                  <button
                    className="btn"
                    onClick={() =>
                      void api.updateTrigger(t.id, { enabled: !t.enabled }).then(load).catch(fail)
                    }
                  >
                    {t.enabled ? "uit" : "aan"}
                  </button>{" "}
                  {!t.builtin && (
                    <button
                      className="btn"
                      onClick={() => void api.updateTrigger(t.id, { remove: true }).then(load).catch(fail)}
                    >
                      ×
                    </button>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </>
  );
}

/// Kleine lijngrafiek (0..1) per maand.
function Spark({ points }: { points: [string, number][] }) {
  if (points.length === 0) return <span style={{ color: "var(--vx-muted)" }}>–</span>;
  const w = 220;
  const h = 48;
  const xs = (i: number) => (points.length === 1 ? w / 2 : (i / (points.length - 1)) * (w - 8) + 4);
  const ys = (v: number) => h - 4 - Math.max(0, Math.min(1, v)) * (h - 8);
  return (
    <svg width={w} height={h} role="img" aria-label="trend">
      <polyline
        fill="none"
        stroke="var(--vx-accent, #7aa2ff)"
        strokeWidth="2"
        points={points.map(([, v], i) => `${xs(i)},${ys(v)}`).join(" ")}
      />
      {points.map(([m, v], i) => (
        <circle key={m} cx={xs(i)} cy={ys(v)} r="3" fill="var(--vx-accent, #7aa2ff)">
          <title>
            {m}: {pct(v)}
          </title>
        </circle>
      ))}
    </svg>
  );
}

function Progress() {
  const [p, setP] = useState<VxProgress | null>(null);
  useEffect(() => {
    api
      .progress()
      .then(setP)
      .catch(() => undefined);
  }, []);
  if (!p) return null;
  const last = p.eval_overall.at(-1);
  return (
    <>
      <div className="row" style={{ gap: 16, flexWrap: "wrap" }}>
        {[
          ["Eval (laatste maand)", last ? pct(last.score) : "–"],
          ["Layers gepromoveerd", String(p.layers_promoted)],
          ["Actieve adapters", String(p.adapters_active)],
          ["Leeragenda klaar / open", `${p.agenda_done} / ${p.agenda_open}`],
          ["Taken afgerond", String(p.tasks_done)],
          ["Wereldmodel Brier", p.world_calibration.n ? p.world_calibration.brier.toFixed(3) : "–"],
        ].map(([k, v]) => (
          <div key={k} className="panel card" style={{ minWidth: 160 }}>
            <div style={{ fontSize: 12, color: "var(--vx-muted)" }}>{k}</div>
            <div style={{ fontSize: 24 }}>{v}</div>
          </div>
        ))}
      </div>
      <div className="panel card" style={{ marginTop: 16 }}>
        <h3>Eval-scores per maand</h3>
        <div className="row" style={{ gap: 8 }}>
          <span style={{ width: 140 }}>gemiddeld</span>
          <Spark points={p.eval_overall.map((x) => [x.month, x.score])} />
        </div>
        {Object.entries(p.eval_by_suite).map(([s, months]) => {
          const tr = p.trend.find((t) => t.suite === s);
          return (
            <div key={s} className="row" style={{ gap: 8 }}>
              <span style={{ width: 140 }}>{s}</span>
              <Spark points={Object.entries(months)} />
              {tr && (
                <span
                  style={{
                    fontSize: 12,
                    color: tr.delta < -0.05 ? "var(--vx-danger, #e66)" : "var(--vx-muted)",
                  }}
                >
                  {tr.delta >= 0 ? "+" : ""}
                  {(tr.delta * 100).toFixed(0)} pt{tr.delta < -0.05 && " → leeragenda"}
                </span>
              )}
            </div>
          );
        })}
      </div>
      <div className="panel card" style={{ marginTop: 16 }}>
        <h3>Geheugengroei per maand</h3>
        <table className="table">
          <tbody>
            {Object.entries(p.memory_by_month).map(([m, kinds]) => (
              <tr key={m}>
                <td>{m}</td>
                <td>
                  {Object.entries(kinds)
                    .map(([k, n]) => `${k} ${n}`)
                    .join(" · ")}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </>
  );
}

function System({ ov }: { ov: AutonomyOverview | null }) {
  const [gpu, setGpu] = useState<VxGpu | null>(null);
  const [bk, setBk] = useState<{ dir: string; backups: VxBackup[] } | null>(null);
  const [busy, setBusy] = useState(false);
  const load = useCallback(() => {
    api
      .gpu()
      .then(setGpu)
      .catch(() => undefined);
    api
      .backups()
      .then(setBk)
      .catch(() => undefined);
  }, []);
  useEffect(load, [load]);
  return (
    <div className="row" style={{ alignItems: "flex-start", gap: 20, flexWrap: "wrap" }}>
      <div className="panel card" style={{ flex: 1, minWidth: 320 }}>
        <h3>GPU-bewaker</h3>
        {gpu && (
          <>
            <p style={{ fontSize: 13 }}>
              {gpu.enabled ? "Aan" : "Uit"} · {gpu.released ? "GPU vrijgegeven" : "modellen geladen"} ·
              terugzetten na {gpu.restore_after_secs} s · gebruiker{" "}
              {gpu.conditions.user_idle ? "idle" : "actief"} · externe GPU-last{" "}
              {gpu.conditions.external_gpu_load ? "ja" : "nee"}
            </p>
            {gpu.servers.length === 0 && (
              <p style={{ fontSize: 12, color: "var(--vx-muted)" }}>
                Geen beheerde servers. Zet <code>managed = true</code> bij een server in models.toml om Vortex
                de llama-server zelf te laten starten en bij gamen te ontladen.
              </p>
            )}
            <ul style={{ fontSize: 13 }}>
              {gpu.servers.map((s) => (
                <li key={s.server}>
                  {s.server}: {s.phase} ({s.model ?? s.home}), {s.inflight} lopend
                </li>
              ))}
            </ul>
          </>
        )}
      </div>
      <div className="panel card" style={{ flex: 1, minWidth: 320 }}>
        <div className="row" style={{ justifyContent: "space-between" }}>
          <h3>Backups</h3>
          <button
            className="btn"
            disabled={busy}
            onClick={() => {
              setBusy(true);
              api
                .backupNow()
                .then(load)
                .catch(fail)
                .finally(() => setBusy(false));
            }}
          >
            {busy ? "bezig…" : "Nu backup + restore-test"}
          </button>
        </div>
        <p style={{ fontSize: 12, color: "var(--vx-muted)" }}>
          {bk?.dir} · terugzetten: kern stoppen, dan{" "}
          <code>vortex restore &lt;bestand&gt; [--with-config]</code>
        </p>
        <table className="table">
          <tbody>
            {bk?.backups.map((b) => (
              <tr key={b.id} title={b.path}>
                <td>{t16(b.created_at)}</td>
                <td>{b.tier}</td>
                <td>{(b.bytes / 1e6).toFixed(1)} MB</td>
                <td>
                  <span className={`pill ${b.verified ? "ok" : ""}`}>
                    {b.verified ? "restore-test ok" : (b.verify.error ?? "niet ok")}
                  </span>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <div className="panel card" style={{ flex: 1, minWidth: 320 }}>
        <h3>API en telefoon</h3>
        <p style={{ fontSize: 13 }}>
          OpenAI-compatibel: <code>{API_BASE}/v1</code> — modellen <code>vortex</code>,{" "}
          <code>vortex-quick</code>, <code>vortex-deep</code>, <code>vortex-research</code> (volledige Vortex)
          of een modelnaam uit models.toml (direct). In Cursor: &quot;OpenAI Base URL&quot; = deze URL,
          willekeurige sleutel.
        </p>
        <p style={{ fontSize: 13 }}>
          Telefoon via Tailscale:{" "}
          {ov?.remote.tailscale ? (
            ov.remote.url ? (
              <code>{ov.remote.url}</code>
            ) : (
              "aan, maar Tailscale geeft geen adres"
            )
          ) : (
            "uit (config/autonomy.toml → [remote] tailscale = true)"
          )}
        </p>
      </div>
    </div>
  );
}
