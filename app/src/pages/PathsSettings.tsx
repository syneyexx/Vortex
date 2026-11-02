// Instellingen → Mappen: paden uit config/paths.toml aanpassen. Ontbrekende mappen geven geen
// crash (Vortex valt terug); hier zie je wat er niet bestaat en pas je het aan.
import { useEffect, useState } from "react";
import { api, type PathsSettings as PS } from "../api/client.ts";
import { setError } from "../state/store.ts";

export function PathsSettings() {
  const [ps, setPs] = useState<PS | null>(null);
  const [draft, setDraft] = useState<Record<string, string>>({});
  const [create, setCreate] = useState(true);
  const [msg, setMsg] = useState<string | null>(null);
  const load = () =>
    api
      .paths()
      .then((p) => {
        setPs(p);
        setDraft(Object.fromEntries(p.items.map((i) => [i.key, i.value])));
      })
      .catch((e: unknown) => setError(String(e)));
  useEffect(() => {
    void load();
  }, []);
  if (!ps) return null;
  const changed = ps.items.filter((i) => (draft[i.key] ?? "") !== i.value);
  return (
    <div className="panel card" style={{ marginTop: 16 }}>
      <h3>Mappen</h3>
      <p style={{ fontSize: 12, color: "var(--vx-muted)" }}>
        Nu in gebruik: data <code>{ps.effective.data_dir}</code> · koud <code>{ps.effective.cold_dir}</code>.
        Ontbreekt een map, dan valt Vortex terug op de datamap.
      </p>
      <table className="table">
        <tbody>
          {ps.items.map((i) => (
            <tr key={i.key}>
              <td style={{ width: 140 }}>
                <code>{i.key}</code>
              </td>
              <td>
                <input
                  value={draft[i.key] ?? ""}
                  onChange={(e) => setDraft({ ...draft, [i.key]: e.target.value })}
                  style={{ width: "100%" }}
                />
                <div style={{ fontSize: 11, color: "var(--vx-muted)" }}>
                  {i.help}
                  {i.restart && " (na herstart)"}
                </div>
              </td>
              <td style={{ width: 110 }}>
                {i.exists === null ? (
                  <span className="pill">–</span>
                ) : i.exists ? (
                  <span className="pill ok">bestaat</span>
                ) : (
                  <span className="pill err">ontbreekt</span>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      <div className="row" style={{ gap: 10, marginTop: 8 }}>
        <label style={{ fontSize: 13 }}>
          <input type="checkbox" checked={create} onChange={(e) => setCreate(e.target.checked)} /> ontbrekende
          mappen aanmaken
        </label>
        <button
          className="btn primary"
          disabled={changed.length === 0}
          onClick={() =>
            void api
              .savePaths(Object.fromEntries(changed.map((i) => [i.key, draft[i.key] ?? ""])), create)
              .then((r) => {
                setMsg(
                  `Opgeslagen (${r.version})${r.created.length ? ` · aangemaakt: ${r.created.join(", ")}` : ""}${changed.some((i) => i.restart) ? " · herstart Vortex voor data_dir/worker_address" : ""}`,
                );
                setPs(r.paths);
              })
              .catch((e: unknown) => setError(String(e)))
          }
        >
          Opslaan
        </button>
        {msg && <span style={{ fontSize: 12 }}>{msg}</span>}
      </div>
    </div>
  );
}
