// Tools & skills beheren (W08): tools met contract, MCP-status, skills (import/export,
// aan/uit, statistiek), voorstellen uit geslaagde taken, herladen.
import { useCallback, useEffect, useState } from "react";
import { api } from "../api/client.ts";
import { setError } from "../state/store.ts";

type Tools = Awaited<ReturnType<typeof api.tools>>;
type Skills = Awaited<ReturnType<typeof api.skills>>;
type Props = Awaited<ReturnType<typeof api.proposals>>;

export function ToolsSettings() {
  const [tools, setTools] = useState<Tools | null>(null);
  const [skills, setSkills] = useState<Skills | null>(null);
  const [props, setProps] = useState<Props>([]);
  const [path, setPath] = useState("");
  const [exportDir, setExportDir] = useState("");
  const [msg, setMsg] = useState<string | null>(null);

  const load = useCallback(() => {
    api
      .tools()
      .then(setTools)
      .catch((e: unknown) => setError(String(e)));
    api
      .skills()
      .then(setSkills)
      .catch(() => undefined);
    api
      .proposals()
      .then(setProps)
      .catch(() => undefined);
  }, []);
  useEffect(load, [load]);

  const run = (p: Promise<unknown>, ok: string) =>
    void p
      .then(() => {
        setMsg(ok);
        load();
      })
      .catch((e: unknown) => setError(String(e)));

  return (
    <>
      <h1 style={{ marginTop: 32 }}>Tools & skills</h1>
      {msg && (
        <div className="pill ok" style={{ marginBottom: 12 }}>
          {msg}
        </div>
      )}
      <div className="row" style={{ alignItems: "flex-start", gap: 20 }}>
        <div className="panel card" style={{ flex: 1 }}>
          <div className="row" style={{ justifyContent: "space-between" }}>
            <h3>Tools ({tools?.tools.length ?? 0})</h3>
            <button className="btn" onClick={() => run(api.reloadTools(), "Herladen")}>
              herladen (skills + MCP)
            </button>
          </div>
          {tools && Object.keys(tools.mcp).length > 0 && (
            <p style={{ fontSize: 13 }}>
              MCP:{" "}
              {Object.entries(tools.mcp)
                .map(([k, v]) => `${k} — ${v}`)
                .join(" · ")}
            </p>
          )}
          <table className="table">
            <tbody>
              {tools?.tools.map((t) => (
                <tr key={t.name}>
                  <td>
                    <code>{t.name}</code>
                    {t.core && (
                      <span className="pill" style={{ marginLeft: 6 }}>
                        kern
                      </span>
                    )}
                  </td>
                  <td>
                    <span
                      className={`pill ${t.effect === "read" ? "ok" : t.effect === "external" ? "err" : "run"}`}
                    >
                      {t.effect === "read" ? "lezen" : t.effect === "local" ? "lokaal" : "extern"}
                    </span>
                  </td>
                  <td style={{ color: "var(--vx-muted)", fontSize: 13 }}>{t.description}</td>
                  <td style={{ fontSize: 12 }}>{t.origin}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        <div className="panel card" style={{ width: 460 }}>
          <h3>Skills</h3>
          <p style={{ color: "var(--vx-muted)", fontSize: 13 }}>
            Map: <code>{skills?.dir}</code> — wijzigingen worden vanzelf geladen. Importeer een Claude-skill
            (map met SKILL.md), Cursor-rules (.mdc of projectmap) of AGENTS.md.
          </p>
          <div className="row">
            <input
              className="btn"
              style={{ flex: 1 }}
              placeholder="pad naar skill / .mdc / AGENTS.md"
              value={path}
              onChange={(e) => setPath(e.target.value)}
            />
            <button
              className="btn primary"
              disabled={!path}
              onClick={() =>
                run(
                  api.importSkill(path).then((r) => setMsg(`Geïmporteerd: ${r.imported.join(", ")}`)),
                  "Geïmporteerd",
                )
              }
            >
              importeren
            </button>
          </div>
          <div className="row" style={{ marginTop: 8 }}>
            <input
              className="btn"
              style={{ flex: 1 }}
              placeholder="exportmap"
              value={exportDir}
              onChange={(e) => setExportDir(e.target.value)}
            />
          </div>
          {skills?.skills.map((s) => {
            const off = s.stats?.status === "disabled";
            const rate =
              s.stats && s.stats.successes + s.stats.failures > 0
                ? Math.round((100 * s.stats.successes) / (s.stats.successes + s.stats.failures))
                : null;
            return (
              <div key={s.name} className="skill-row" style={{ opacity: off ? 0.5 : 1 }}>
                <div>
                  <strong>{s.name}</strong> <span className="pill">{s.origin}</span>{" "}
                  {s.always_apply && <span className="pill">altijd</span>}
                  <div style={{ color: "var(--vx-muted)", fontSize: 13 }}>{s.description}</div>
                  <div style={{ fontSize: 12 }}>
                    {s.stats
                      ? `${s.stats.uses}× gebruikt${rate !== null ? ` · ${rate}% geslaagd` : ""}`
                      : "nog niet gebruikt"}
                    {s.globs.length > 0 && ` · globs: ${s.globs.join(", ")}`}
                    {s.scripts.length > 0 && ` · scripts: ${s.scripts.join(", ")}`}
                  </div>
                </div>
                <div className="row" style={{ gap: 4, flexWrap: "wrap" }}>
                  <button
                    className="btn"
                    onClick={() =>
                      run(api.skillStatus(s.name, off ? "active" : "disabled"), off ? "Aangezet" : "Uitgezet")
                    }
                  >
                    {off ? "aan" : "uit"}
                  </button>
                  {(["claude", "cursor", "agents"] as const).map((f) => (
                    <button
                      key={f}
                      className="btn"
                      disabled={!exportDir}
                      onClick={() =>
                        run(
                          api
                            .exportSkill(s.name, f, exportDir)
                            .then((r) => setMsg(`Geëxporteerd naar ${r.path}`)),
                          "Geëxporteerd",
                        )
                      }
                    >
                      → {f}
                    </button>
                  ))}
                  <button className="btn" onClick={() => run(api.deleteSkill(s.name), "Verwijderd")}>
                    verwijderen
                  </button>
                </div>
              </div>
            );
          })}
          <h3 style={{ marginTop: 20 }}>Voorstellen</h3>
          <p style={{ color: "var(--vx-muted)", fontSize: 13 }}>
            Uit geslaagde taken met meerdere toolstappen.
          </p>
          {props.length === 0 && <div className="empty">Nog geen voorstellen.</div>}
          {props.map((p) => (
            <details key={p.id} className="skill-row">
              <summary>
                <strong>{p.name}</strong> · {p.seen}× gezien — {p.description}
              </summary>
              <pre>{p.body}</pre>
              <div className="row">
                <button
                  className="btn primary"
                  onClick={() => run(api.decideProposal(p.id, "accepted"), "Skill aangemaakt")}
                >
                  accepteren
                </button>
                <button
                  className="btn"
                  onClick={() => run(api.decideProposal(p.id, "rejected"), "Afgewezen")}
                >
                  afwijzen
                </button>
              </div>
            </details>
          ))}
        </div>
      </div>
    </>
  );
}
