// Memory-pagina: bekijken, zoeken, filteren, bewerken, vergeten, conflictregister, werkruimtes,
// "zoek als Vortex" (met scoreherkomst), backup / export / import. VX:W05.13
import { useCallback, useEffect, useState } from "react";
import { api } from "../api/client.ts";
import type { ConflictRow, Memory, Retrieved } from "../api/types.ts";
import { setError } from "../state/store.ts";

const KINDS = [
  "",
  "user",
  "semantic",
  "episodic",
  "project",
  "procedural",
  "summary",
  "world",
  "long_term",
  "short_term",
];

function Snippet({ m }: { m: Memory }) {
  return (
    <div>
      <div style={{ fontSize: 14 }}>{m.title || m.content.slice(0, 90)}</div>
      <div style={{ fontSize: 12, color: "var(--vx-muted)" }}>
        {m.source_uri ?? m.provenance.origin ?? m.provenance.source}
        {m.locator ? ` · ${m.locator}` : ""} · v{m.version}
        {m.merged_count > 1 ? ` · ${m.merged_count}× samengevoegd` : ""}
      </div>
    </div>
  );
}

function Detail({ id, onDone }: { id: string; onDone: () => void }) {
  const [m, setM] = useState<Memory | null>(null);
  const [versions, setVersions] = useState<[number, string, string, string][]>([]);
  const [draft, setDraft] = useState("");
  useEffect(() => {
    api
      .memory(id)
      .then((r) => {
        setM(r.memory);
        setVersions(r.versions);
        setDraft(r.memory.content);
      })
      .catch((e: unknown) => setError(String(e)));
  }, [id]);
  if (!m) return null;
  return (
    <div className="panel card">
      <div className="row">
        <span className="pill">{m.kind}</span>
        {m.stale && <span className="pill err">verouderd</span>}
        {m.valid_to && <span className="pill">historie</span>}
        <span className="pill">belang {(m.importance * 100).toFixed(0)}%</span>
        <span className="pill">vertrouwen {(m.confidence * 100).toFixed(0)}%</span>
        <span className="pill">{m.access_count}× gebruikt</span>
      </div>
      {m.context && <p style={{ color: "var(--vx-muted)", fontSize: 13 }}>Context: {m.context}</p>}
      <textarea
        className="editor"
        style={{ minHeight: 140 }}
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
      />
      <div className="row" style={{ marginTop: 10 }}>
        <button
          className="btn primary"
          disabled={draft === m.content}
          onClick={() =>
            void api
              .editMemory(id, draft)
              .then(onDone)
              .catch((e: unknown) => setError(String(e)))
          }
        >
          Opslaan (nieuwe versie)
        </button>
        <button
          className="btn danger"
          onClick={() => {
            if (
              !confirm(
                "Vergeten? Ook alles wat ervan is afgeleid (samenvattingen) verdwijnt, uit alle indexen.",
              )
            )
              return;
            void api
              .forget(id)
              .then(onDone)
              .catch((e: unknown) => setError(String(e)));
          }}
        >
          Vergeet dit
        </button>
      </div>
      {versions.length > 0 && (
        <details style={{ marginTop: 10 }}>
          <summary>Eerdere versies ({versions.length})</summary>
          {versions.map(([v, c, reason, at]) => (
            <div
              key={v}
              style={{ fontSize: 13, padding: "6px 0", borderBottom: "1px solid var(--vx-border)" }}
            >
              v{v} · {new Date(at).toLocaleString("nl-NL")} · {reason}
              <div style={{ color: "var(--vx-muted)" }}>{c}</div>
            </div>
          ))}
        </details>
      )}
    </div>
  );
}

function Conflicts() {
  const [rows, setRows] = useState<ConflictRow[]>([]);
  const load = useCallback(() => {
    api
      .conflicts()
      .then(setRows)
      .catch((e: unknown) => setError(String(e)));
  }, []);
  useEffect(load, [load]);
  if (rows.length === 0)
    return (
      <div className="empty">
        Geen conflicten. Botsende feiten verschijnen hier; het conflict-team beslist ze vanzelf (in een
        gesprek direct, anders op de achtergrond).
      </div>
    );
  return (
    <>
      {rows.map((r) => (
        <div className="panel card" key={r.conflict.id}>
          <div className="row">
            <span
              className={`pill ${r.conflict.status === "open" ? "run" : r.conflict.status === "disputed" ? "err" : "ok"}`}
            >
              {r.conflict.status}
            </span>
            <span>{r.conflict.reason}</span>
          </div>
          <div className="row" style={{ alignItems: "stretch", marginTop: 10 }}>
            {[r.a, r.b].map((m, i) => (
              <div key={i} className="panel card" style={{ flex: 1, margin: 0 }}>
                {m ? <Snippet m={m} /> : <em>vergeten</em>}
                <p style={{ fontSize: 13 }}>{m?.content}</p>
                {r.conflict.status === "open" && m && (
                  <button
                    className="btn"
                    onClick={() => void api.resolveConflict(r.conflict.id, m.id, "gekozen in GUI").then(load)}
                  >
                    Deze klopt
                  </button>
                )}
              </div>
            ))}
          </div>
          {r.conflict.status === "open" && (
            <button
              className="btn"
              style={{ marginTop: 8 }}
              onClick={() => void api.resolveConflict(r.conflict.id, null, "onbeslist").then(load)}
            >
              Markeer als betwist
            </button>
          )}
          {r.conflict.status === "open" && (
            <button
              className="btn primary"
              style={{ marginTop: 8, marginLeft: 8 }}
              onClick={() =>
                void api
                  .teamConflict(r.conflict.id)
                  .then(load)
                  .catch((e: unknown) => setError(String(e)))
              }
            >
              Laat het conflict-team beslissen
            </button>
          )}
          {r.conflict.resolution && (
            <p style={{ color: "var(--vx-muted)", fontSize: 13 }}>{r.conflict.resolution}</p>
          )}
          {r.conflict.status !== "open" && (
            <button className="btn" onClick={() => void api.rollbackConflict(r.conflict.id).then(load)}>
              Beslissing terugdraaien
            </button>
          )}
        </div>
      ))}
    </>
  );
}

function Documents() {
  const [docs, setDocs] = useState<Awaited<ReturnType<typeof api.documents>>>([]);
  const [path, setPath] = useState("");
  const [msg, setMsg] = useState<string | null>(null);
  const load = useCallback(() => {
    api
      .documents()
      .then(setDocs)
      .catch((e: unknown) => setError(String(e)));
  }, []);
  useEffect(load, [load]);
  const go = async () => {
    const r = await api.ingest(
      path
        .split(";")
        .map((p) => p.trim())
        .filter(Boolean),
    );
    setMsg(
      r.done && r.report
        ? `${r.report.docs} document(en), ${r.report.sections} secties, ${r.report.skipped} ongewijzigd, ${r.report.summaries} samenvattingen${r.report.via_worker ? "" : " (zonder Python-worker: alleen tekst/Markdown/HTML/code)"}${r.report.errors.length ? ` — fouten: ${r.report.errors.join("; ")}` : ""}`
        : "Gestart op de achtergrond — voortgang in Lopend werk.",
    );
    load();
  };
  return (
    <div className="panel card">
      <h3>Documenten inlezen</h3>
      <p style={{ color: "var(--vx-muted)", fontSize: 13 }}>
        Bestanden of mappen (EPUB, PDF, DOCX, Markdown, HTML, code, tekst). Meerdere paden scheiden met ;.
        Ongewijzigde bestanden worden overgeslagen. Antwoorden verwijzen naar hoofdstuk/pagina/regel.
      </p>
      <div className="row">
        <input
          className="btn"
          style={{ flex: 1 }}
          placeholder="D:/Boeken of C:/Projecten/notities"
          value={path}
          onChange={(e) => setPath(e.target.value)}
        />
        <button
          className="btn primary"
          disabled={!path.trim()}
          onClick={() => void go().catch((e: unknown) => setError(String(e)))}
        >
          Inlezen
        </button>
      </div>
      {msg && <p style={{ fontSize: 13 }}>{msg}</p>}
      <table className="table">
        <tbody>
          {docs.map((d) => (
            <tr key={d.uri}>
              <td>{d.title ?? d.uri}</td>
              <td style={{ fontSize: 12, color: "var(--vx-muted)" }}>{d.uri}</td>
              <td>{d.pieces} stukken</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function Profile() {
  const [p, setP] = useState<Awaited<ReturnType<typeof api.profile>> | null>(null);
  useEffect(() => {
    api
      .profile()
      .then(setP)
      .catch((e: unknown) => setError(String(e)));
  }, []);
  if (!p) return null;
  return (
    <div className="panel card">
      <h3>Wat Vortex over jou weet</h3>
      <p style={{ color: "var(--vx-muted)", fontSize: 13 }}>
        Afgeleid uit je gebruikersgeheugen en feedback; gaat mee in elke prompt (stijl en toon). Aanpassen =
        de herinnering bewerken of vergeten.
      </p>
      <h4>Stijl</h4>
      {p.profile.style.length ? (
        p.profile.style.map((s) => <div key={s}>• {s}</div>)
      ) : (
        <em>Geen voorkeuren bekend. Zeg bijvoorbeeld: "Ik wil graag korte antwoorden."</em>
      )}
      <h4>Feiten</h4>
      {p.profile.facts.map((s) => (
        <div key={s}>• {s}</div>
      ))}
      <h4>Doelen</h4>
      {p.profile.goals.map((s) => (
        <div key={s}>• {s}</div>
      ))}
      <h4>Feedback</h4>
      <div>
        {Object.entries(p.profile.feedback)
          .map(([k, v]) => `${k}: ${v}`)
          .join(" · ")}
      </div>
    </div>
  );
}

export function MemoryPage() {
  const [tab, setTab] = useState<"list" | "search" | "conflicts" | "profile" | "docs">("list");
  const [q, setQ] = useState("");
  const [kind, setKind] = useState("");
  const [ws, setWs] = useState("");
  const [stale, setStale] = useState(false);
  const [items, setItems] = useState<Memory[]>([]);
  const [hits, setHits] = useState<Retrieved[]>([]);
  const [sel, setSel] = useState<string | null>(null);
  const [stats, setStats] = useState<Record<string, unknown>>({});
  const [wss, setWss] = useState<string[]>([]);
  const [newText, setNewText] = useState("");
  const [msg, setMsg] = useState<string | null>(null);

  const load = useCallback(() => {
    api
      .memories({ q, kind, workspace: ws, stale, limit: 100 })
      .then(setItems)
      .catch((e: unknown) => setError(String(e)));
    api
      .memoryStats()
      .then(setStats)
      .catch(() => undefined);
    api
      .workspaces()
      .then(setWss)
      .catch(() => undefined);
  }, [q, kind, ws, stale]);
  useEffect(load, [load]);

  const search = () => {
    api
      .searchMemory(q, ws)
      .then(setHits)
      .catch((e: unknown) => setError(String(e)));
  };

  return (
    <div className="page">
      <div className="row">
        <h1 style={{ margin: 0 }}>Memory</h1>
        <div className="row" style={{ marginLeft: "auto", gap: 8 }}>
          <span className="pill">{String(stats.memories ?? 0)} herinneringen</span>
          <span className="pill">{String(stats.entities ?? 0)} entiteiten</span>
          <span className="pill">{String(stats.edges ?? 0)} relaties</span>
          <span className={`pill ${Number(stats.open_conflicts ?? 0) > 0 ? "run" : ""}`}>
            {String(stats.open_conflicts ?? 0)} conflicten
          </span>
          <span className={`pill ${stats.reranker ? "ok" : ""}`}>
            reranker {stats.reranker ? "aan" : "uit"}
          </span>
          <button
            className="btn"
            onClick={() =>
              void api
                .backup()
                .then((r) => setMsg(`Backup: ${r.path}`))
                .catch((e: unknown) => setError(String(e)))
            }
          >
            Backup
          </button>
          <a className="btn" href={api.exportUrl()} download>
            Export
          </a>
          <label className="btn" style={{ cursor: "pointer" }}>
            Import
            <input
              type="file"
              accept=".jsonl,.ndjson,.txt"
              hidden
              onChange={async (e) => {
                const f = e.target.files?.[0];
                if (!f) return;
                const r = await api.importMemory(await f.text());
                setMsg(r.error ? `Import mislukt: ${r.error}` : `${r.imported ?? 0} geïmporteerd`);
                load();
              }}
            />
          </label>
        </div>
      </div>
      {msg && <p style={{ color: "var(--vx-muted)" }}>{msg}</p>}
      <div className="chat-tabs" style={{ display: "inline-flex", margin: "16px 0" }}>
        {(["list", "search", "docs", "conflicts", "profile"] as const).map((t) => (
          <button key={t} className={tab === t ? "active" : ""} onClick={() => setTab(t)}>
            {
              {
                list: "Bekijken",
                search: "Zoek als Vortex",
                docs: "Documenten",
                conflicts: "Conflictregister",
                profile: "Over jou",
              }[t]
            }
          </button>
        ))}
      </div>
      {(tab === "list" || tab === "search") && (
        <div className="row" style={{ marginBottom: 12 }}>
          <input
            className="btn"
            style={{ flex: 1 }}
            placeholder={tab === "search" ? "Vraag zoals in de chat…" : "Zoek in tekst…"}
            value={q}
            onChange={(e) => setQ(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && tab === "search" && search()}
          />
          {tab === "list" && (
            <select className="btn" value={kind} onChange={(e) => setKind(e.target.value)}>
              {KINDS.map((k) => (
                <option key={k} value={k}>
                  {k || "alle typen"}
                </option>
              ))}
            </select>
          )}
          <select className="btn" value={ws} onChange={(e) => setWs(e.target.value)}>
            <option value="">algemeen</option>
            {wss.map((w) => (
              <option key={w}>{w}</option>
            ))}
          </select>
          {tab === "list" && (
            <label className="row" style={{ gap: 6, fontSize: 13 }}>
              <input type="checkbox" checked={stale} onChange={(e) => setStale(e.target.checked)} /> ook
              verouderd
            </label>
          )}
          {tab === "search" && (
            <button className="btn primary" onClick={search}>
              Zoek
            </button>
          )}
        </div>
      )}
      {tab === "list" && (
        <div className="row" style={{ alignItems: "flex-start", gap: 16 }}>
          <div style={{ flex: 1 }}>
            <div className="panel card">
              <div className="row">
                <input
                  className="btn"
                  style={{ flex: 1 }}
                  placeholder="Onthoud iets… (wordt als gebruikersfeit opgeslagen)"
                  value={newText}
                  onChange={(e) => setNewText(e.target.value)}
                />
                <button
                  className="btn primary"
                  disabled={!newText.trim()}
                  onClick={() =>
                    void api.remember(newText, "user", ws).then(() => {
                      setNewText("");
                      load();
                    })
                  }
                >
                  Onthoud
                </button>
              </div>
            </div>
            <div className="panel card">
              <table className="table">
                <tbody>
                  {items.map((m) => (
                    <tr
                      key={m.id}
                      onClick={() => setSel(m.id)}
                      style={{
                        cursor: "pointer",
                        background: sel === m.id ? "var(--vx-panel-hi)" : undefined,
                      }}
                    >
                      <td>
                        <span className="pill">{m.kind}</span>
                      </td>
                      <td>
                        <Snippet m={m} />
                      </td>
                      <td style={{ whiteSpace: "nowrap", color: "var(--vx-muted)" }}>
                        {new Date(m.updated_at).toLocaleDateString("nl-NL")}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
              {items.length === 0 && (
                <div className="empty">
                  Nog geen herinneringen. Vortex onthoudt vanzelf belangrijke dingen uit gesprekken.
                </div>
              )}
            </div>
          </div>
          <div style={{ width: 520 }}>
            {sel && (
              <Detail
                key={sel}
                id={sel}
                onDone={() => {
                  setSel(null);
                  load();
                }}
              />
            )}
          </div>
        </div>
      )}
      {tab === "search" && (
        <div className="panel card">
          {hits.map((h) => (
            <div key={h.memory.id} style={{ padding: "10px 0", borderBottom: "1px solid var(--vx-border)" }}>
              <div className="row">
                <span className="pill">{h.memory.kind}</span>
                {h.counter_evidence ? (
                  <span className="pill err">tegenbewijs</span>
                ) : (
                  <span className="score">{(h.score.final_score * 100).toFixed(0)}%</span>
                )}
                <span style={{ fontSize: 12, color: "var(--vx-muted)" }}>
                  Waarom:{" "}
                  {[
                    h.score.vector != null && `vector ${(h.score.vector * 100).toFixed(0)}%`,
                    h.score.bm25_rank != null && `trefwoord #${h.score.bm25_rank}`,
                    h.score.graph_hops != null && `graph ${h.score.graph_hops} hop`,
                    h.score.rerank != null && `rerank ${(h.score.rerank * 100).toFixed(0)}%`,
                    h.score.temporal && "tijdfilter",
                    `recentheid ${(h.score.recency * 100).toFixed(0)}%`,
                    `belang ${(h.score.importance * 100).toFixed(0)}%`,
                  ]
                    .filter(Boolean)
                    .join(" · ")}
                </span>
              </div>
              <p style={{ margin: "6px 0 0", fontSize: 14 }}>{h.memory.content}</p>
            </div>
          ))}
          {hits.length === 0 && (
            <div className="empty">Typ een vraag en zie wat Vortex zou ophalen — en waarom.</div>
          )}
        </div>
      )}
      {tab === "conflicts" && <Conflicts />}
      {tab === "profile" && <Profile />}
      {tab === "docs" && <Documents />}
    </div>
  );
}
