// Pagina's buiten de chat. Functies die later komen tonen een lege staat — nooit nep-data.
import { useEffect, useState } from "react";
import { ToolsSettings } from "./ToolsSettings.tsx";
import { api } from "../api/client.ts";
import type { ModelInfo, RunRecord } from "../api/types.ts";
import { Inspector } from "../components/Inspector.tsx";
import { setError, useStore } from "../state/store.ts";

export function Placeholder({ title, wave, text }: { title: string; wave: string; text: string }) {
  return (
    <div className="page">
      <h1>{title}</h1>
      <div className="panel card empty">
        <div>
          <h2>Komt in {wave}</h2>
          <p>{text}</p>
        </div>
      </div>
    </div>
  );
}

export function RunsPage() {
  const [runs, setRuns] = useState<{ active: RunRecord[]; recent: RunRecord[] } | null>(null);
  const [insp, setInsp] = useState<string | null>(null);
  const live = useStore((s) => s.lastSeq);
  useEffect(() => {
    api
      .runs()
      .then(setRuns)
      .catch((e: unknown) => setError(String(e)));
  }, [live]);
  const stopAll = async () => {
    for (const r of runs?.active ?? []) await api.stop(r.id);
  };
  const row = (r: RunRecord, stop: boolean) => (
    <tr key={r.id}>
      <td>
        <button className="fork" title="Trace" onClick={() => setInsp(r.id)}>
          <code>{r.id.slice(0, 14)}…</code>
        </button>
      </td>
      <td>{r.kind}</td>
      <td>
        <span className={`pill ${r.state === "completed" ? "ok" : r.state === "failed" ? "err" : "run"}`}>
          {r.state}
        </span>
      </td>
      <td>{r.priority}</td>
      <td>{new Date(r.created_at).toLocaleString("nl-NL")}</td>
      <td>
        {r.usage.tokens} tok · {r.usage.steps} stappen
      </td>
      <td>{r.error ?? ""}</td>
      <td>
        {stop && (
          <button className="btn danger" onClick={() => void api.stop(r.id)}>
            Stop
          </button>
        )}
      </td>
    </tr>
  );
  return (
    <div className="page">
      <div className="row">
        <h1 style={{ margin: 0 }}>Lopend werk</h1>
        <button
          className="btn danger"
          style={{ marginLeft: "auto" }}
          disabled={!runs?.active.length}
          onClick={() => void stopAll()}
        >
          Alles stoppen
        </button>
      </div>
      <DirigentCard />
      {insp && <Inspector runId={insp} onClose={() => setInsp(null)} />}
      <div className="panel card">
        <h3>Actief</h3>
        <table className="table">
          <tbody>{runs?.active.map((r) => row(r, true))}</tbody>
        </table>
        {runs?.active.length === 0 && <div className="empty">Niets actief.</div>}
      </div>
      <div className="panel card">
        <h3>Recent</h3>
        <table className="table">
          <tbody>{runs?.recent.map((r) => row(r, false))}</tbody>
        </table>
      </div>
    </div>
  );
}

function DirigentCard() {
  const d = useStore((s) => (s.status as unknown as { dirigent?: DirigentSnap } | null)?.dirigent);
  if (!d) return null;
  const bar = (used: number, cap: number) => `${used} / ${cap}`;
  return (
    <div className="panel card">
      <h3>Dirigent</h3>
      <div className="row" style={{ flexWrap: "wrap", gap: 8 }}>
        <span className="pill">hoofd-GPU {bar(d.used.main_gpu, d.capacity.main_gpu)}</span>
        <span className="pill">1060 {bar(d.used.aux_gpu_mib, d.capacity.aux_gpu_mib)} MiB</span>
        <span className="pill">cores {bar(d.used.cores, d.capacity.cores)}</span>
        <span className="pill">RAM {bar(d.used.ram_mib, d.capacity.ram_mib)} MiB</span>
        <span className={`pill ${d.conditions.user_idle ? "ok" : ""}`}>
          gebruiker {d.conditions.user_idle ? "idle" : "actief"}
        </span>
        <span className={`pill ${d.conditions.external_gpu_load ? "err" : ""}`}>
          externe GPU-last {d.conditions.external_gpu_load ? "ja" : "nee"}
        </span>
        <span className={`pill ${d.conditions.pressure ? "err" : "ok"}`}>
          druk {["normaal", "hoog", "kritiek"][d.conditions.pressure]}
        </span>
      </div>
      <p style={{ color: "var(--vx-muted)", fontSize: 13 }}>
        Lopend: {d.running.map((r) => `${r[0]} (${r[2]})`).join(", ") || "—"} · Wachtend:{" "}
        {d.waiting.map((r) => `${r[0]} (${r[2]})`).join(", ") || "—"}
      </p>
    </div>
  );
}

interface DemandSnap {
  main_gpu: number;
  aux_gpu_mib: number;
  cores: number;
  ram_mib: number;
}
interface DirigentSnap {
  capacity: DemandSnap;
  used: DemandSnap;
  conditions: { user_idle: boolean; external_gpu_load: boolean; pressure: number };
  running: [string, string, string][];
  waiting: [string, string, string][];
}

export function ModelsPage() {
  const [data, setData] = useState<{ default: string; models: ModelInfo[] } | null>(null);
  const st = useStore((s) => s.status);
  useEffect(() => {
    api
      .models()
      .then(setData)
      .catch((e: unknown) => setError(String(e)));
  }, []);
  return (
    <div className="page">
      <h1>Models</h1>
      <div className="panel card">
        <h3>Servers</h3>
        <table className="table">
          <thead>
            <tr>
              <th>Naam</th>
              <th>Rol</th>
              <th>URL</th>
              <th>Model</th>
              <th>Status</th>
            </tr>
          </thead>
          <tbody>
            {st?.servers.map((s) => (
              <tr key={s.name}>
                <td>{s.name}</td>
                <td>{s.role}</td>
                <td>{s.url}</td>
                <td>{s.model}</td>
                <td>
                  <span className={`pill ${s.ready ? "ok" : "err"}`}>{s.detail}</span>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <div className="panel card">
        <h3>Registry (config/models.toml)</h3>
        <table className="table">
          <thead>
            <tr>
              <th>Model</th>
              <th>Rollen</th>
              <th>Server</th>
              <th>Context</th>
              <th>KV</th>
              <th>Parser</th>
              <th>Bestand</th>
            </tr>
          </thead>
          <tbody>
            {data?.models.map((m) => (
              <tr key={m.name}>
                <td>
                  {m.name}
                  {m.name === data.default && (
                    <span className="pill run" style={{ marginLeft: 6 }}>
                      standaard
                    </span>
                  )}
                </td>
                <td>{m.roles.join(", ")}</td>
                <td>{m.server}</td>
                <td>{m.context ?? "—"}</td>
                <td>{m.kv_type ?? "—"}</td>
                <td>{m.parser}</td>
                <td>
                  <code>{m.file}</code>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

export function SettingsPage() {
  const [files, setFiles] = useState<{ path: string; content: string }[]>([]);
  const [version, setVersion] = useState("");
  const [path, setPath] = useState("prompts/system.md");
  const [draft, setDraft] = useState("");
  const [history, setHistory] = useState<{ version: string; ts: string }[]>([]);
  const [saved, setSaved] = useState<string | null>(null);

  const load = async () => {
    const p = await api.prompts();
    setFiles(p.files);
    setVersion(p.version);
    setHistory(await api.history());
    return p;
  };
  useEffect(() => {
    load()
      .then((p) => setDraft(p.files.find((f) => f.path === "prompts/system.md")?.content ?? ""))
      .catch((e: unknown) => setError(String(e)));
  }, []);

  const pick = (p: string) => {
    setPath(p);
    setDraft(files.find((f) => f.path === p)?.content ?? "");
  };
  const save = async () => {
    const r = await api.savePrompt(path, draft);
    setSaved(
      `Opgeslagen als ${r.version} — actief vanaf het volgende bericht. Lopende antwoorden houden hun versie.`,
    );
    await load();
  };
  const restore = async (v: string) => {
    await api.restore(v, path);
    const p = await load();
    setDraft(p.files.find((f) => f.path === path)?.content ?? "");
    setSaved(`Teruggezet naar ${v}.`);
  };

  return (
    <div className="page">
      <h1>System prompt & rollen</h1>
      <div className="row" style={{ alignItems: "flex-start", gap: 20 }}>
        <div className="panel card" style={{ flex: 1 }}>
          <div className="row">
            <select className="btn" value={path} onChange={(e) => pick(e.target.value)}>
              {[...new Set(["prompts/system.md", ...files.map((f) => f.path)])].map((p) => (
                <option key={p}>{p}</option>
              ))}
            </select>
            <span className="pill">config {version}</span>
          </div>
          <textarea
            className="editor"
            style={{ marginTop: 12 }}
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
          />
          <div className="row" style={{ marginTop: 12 }}>
            <button className="btn primary" onClick={() => void save()}>
              Opslaan (live vanaf volgend bericht)
            </button>
            {saved && <span style={{ color: "var(--vx-muted)", fontSize: 13 }}>{saved}</span>}
          </div>
        </div>
        <div className="panel card" style={{ width: 360 }}>
          <h3>Versiegeschiedenis</h3>
          {history.map((h) => (
            <div
              className="row"
              key={h.version}
              style={{ justifyContent: "space-between", padding: "6px 0" }}
            >
              <span>
                <code>{h.version.slice(0, 12)}</code>{" "}
                <span style={{ color: "var(--vx-muted)", fontSize: 12 }}>
                  {new Date(h.ts).toLocaleString("nl-NL")}
                </span>
              </span>
              <button
                className="btn"
                disabled={h.version === version}
                onClick={() => void restore(h.version)}
              >
                terugzetten
              </button>
            </div>
          ))}
        </div>
      </div>
      <ToolsSettings />
      <Placeholder
        title=""
        wave="W04 / W07"
        text="Paden, modellen en gedrag als formulier; nu via config/*.toml (hot-reload)."
      />
    </div>
  );
}
