// Midden: kop + tabs, gesprek, achtergrondtaak-balk, invoerbalk.
import { useEffect, useMemo, useRef, useState } from "react";
import { api } from "../api/client.ts";
import type { ChatMessage, Mode, ModelInfo } from "../api/types.ts";
import { getState, newSession, openSession, setError, useStore } from "../state/store.ts";
import { Composer } from "./Composer.tsx";
import { IcClock, IcFork, IcPlus, IcUser } from "./Icons.tsx";
import { Inspector } from "./Inspector.tsx";
import { Markdown } from "./Markdown.tsx";
import { ToolCards } from "./ToolCards.tsx";
import { Replay } from "./Replay.tsx";
import { TaskBar } from "./TaskBar.tsx";
import { VortexMark } from "./VortexShader.tsx";

const EMPTY: ChatMessage[] = [];

function time(ts: string): string {
  const d = new Date(ts);
  const today = new Date().toDateString() === d.toDateString();
  const hm = d.toLocaleTimeString("nl-NL", { hour: "2-digit", minute: "2-digit" });
  return today ? `Vandaag ${hm}` : `${d.toLocaleDateString("nl-NL")} ${hm}`;
}

function Feedback({ m }: { m: ChatMessage }) {
  const [given, setGiven] = useState<string | null>(null);
  const send = async (rating: "good" | "bad" | "correction") => {
    let correction: string | undefined;
    if (rating === "correction") {
      correction = prompt("Wat is het juiste antwoord? (wordt als feit onthouden)") ?? undefined;
      if (!correction) return;
    }
    await api.feedback(m.id, rating, correction);
    setGiven(rating);
  };
  return (
    <span className="msg-actions">
      <button className={given === "good" ? "on" : ""} title="Goed" onClick={() => void send("good")}>
        👍
      </button>
      <button className={given === "bad" ? "on" : ""} title="Fout" onClick={() => void send("bad")}>
        👎
      </button>
      <button
        className={given === "correction" ? "on" : ""}
        title="Correctie"
        onClick={() => void send("correction")}
      >
        ✎
      </button>
    </span>
  );
}

function Assistant({ m, sessionId }: { m: ChatMessage; sessionId: string }) {
  const [inspect, setInspect] = useState(false);
  const stat = useStore((s) => s.streamStats[m.id]);
  const streaming = m.status === "streaming";
  const tps = m.metrics?.metrics?.decode_tps;
  const live =
    stat && streaming ? stat.chars / 4 / Math.max(0.25, (Date.now() - stat.startedAt) / 1000) : undefined;
  const fork = async () => {
    const f = await api.fork(sessionId, m.id);
    await openSession(f.id);
  };
  return (
    <div className="msg-vx">
      <div className="avatar">
        <VortexMark size={64} />
      </div>
      <div>
        <div className="msg-meta">
          <span className="who">Vortex</span>
          <span className="when">{time(m.created_at)}</span>
          {m.model && <span className="when">· {m.model}</span>}
          {!streaming && <Feedback m={m} />}
          {!streaming && m.run_id && (
            <button
              className="fork"
              style={{ marginLeft: 6 }}
              title="Context + trace"
              onClick={() => setInspect(true)}
            >
              inspector
            </button>
          )}
          {!streaming && (
            <button className="fork" title="Gesprek vanaf hier forken" onClick={() => void fork()}>
              <IcFork size={14} /> fork
            </button>
          )}
        </div>
        {m.reasoning && (
          <details className="reasoning" open={streaming && !m.content}>
            <summary>Redenering</summary>
            {m.reasoning}
          </details>
        )}
        {m.run_id && <ToolCards runId={m.run_id} />}
        <Markdown text={m.content} streaming={streaming} />
        {streaming && (
          <div className="writing">
            <span className="dots">
              <i />
              <i />
              <i />
            </span>
            {m.content ? "Writing response" : m.reasoning ? "Denkt na" : "Wacht op model"}
            {live !== undefined && live > 0 && <span>· ~{live.toFixed(0)} tok/s</span>}
            <span className="cursor" />
          </div>
        )}
        {!streaming && tps !== undefined && <div className="writing">{tps.toFixed(1)} tok/s</div>}
        {m.status === "cancelled" && <div className="msg-cancel">Gestopt.</div>}
        {m.status === "failed" && <div className="msg-error">Mislukt{m.error ? `: ${m.error}` : ""}</div>}
        {inspect && m.run_id && <Inspector runId={m.run_id} onClose={() => setInspect(false)} />}
      </div>
    </div>
  );
}

function History({ onClose }: { onClose: () => void }) {
  const sessions = useStore((s) => s.sessions);
  const current = useStore((s) => s.currentSession);
  return (
    <div className="panel history" onMouseLeave={onClose}>
      {sessions.length === 0 && <div className="empty">Nog geen gesprekken.</div>}
      {sessions.map((s) => (
        <button
          key={s.id}
          className={s.id === current ? "active" : ""}
          onClick={() => {
            void openSession(s.id);
            onClose();
          }}
        >
          <div className="t">{s.title}</div>
          <div className="d">
            {new Date(s.updated_at).toLocaleString("nl-NL")} · {s.message_count} berichten
            {s.parent_session_id ? " · fork" : ""}
          </div>
        </button>
      ))}
    </div>
  );
}

export function ChatPane() {
  const current = useStore((s) => s.currentSession);
  const session = useStore((s) => s.sessions.find((x) => x.id === s.currentSession));
  const messages = useStore((s) => (s.currentSession ? s.messages[s.currentSession] : undefined) ?? EMPTY);
  const [tab, setTab] = useState<"conversation" | "sources" | "artifacts">("conversation");
  const [showHistory, setShowHistory] = useState(false);
  const [replay, setReplay] = useState(false);
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [defaultModel, setDefaultModel] = useState("");
  const scroller = useRef<HTMLDivElement>(null);
  const stick = useRef(true);

  useEffect(() => {
    api
      .models()
      .then((r) => {
        setModels(r.models.filter((m) => m.chat));
        setDefaultModel(r.default);
      })
      .catch((e: unknown) => setError(String(e)));
  }, []);

  // Blijf onderaan tijdens streamen, tenzij de gebruiker omhoog scrolde.
  useEffect(() => {
    const el = scroller.current;
    if (el && stick.current) el.scrollTop = el.scrollHeight;
  }, [messages]);

  const activeRun = useMemo(() => messages.find((m) => m.status === "streaming")?.run_id ?? null, [messages]);

  const send = async (text: string, mode: Mode, model: string) => {
    let sid = getState().currentSession;
    if (!sid) sid = await newSession();
    stick.current = true;
    await api.send(sid, text, mode, model || undefined);
  };

  return (
    <section className="panel chat" style={{ position: "relative" }}>
      <header className="chat-head">
        <div>
          <h1>{session?.title ?? "Nieuwe chat"}</h1>
          <div className="sub">
            {session
              ? `${session.message_count} berichten · lokaal opgeslagen`
              : "Stel een vraag of geef een taak."}
          </div>
        </div>
        <div className="chat-tabs">
          <button className={tab === "conversation" ? "active" : ""} onClick={() => setTab("conversation")}>
            Conversation
          </button>
          <button className={tab === "sources" ? "active" : ""} onClick={() => setTab("sources")}>
            Sources<span className="badge">0</span>
          </button>
          <button className={tab === "artifacts" ? "active" : ""} onClick={() => setTab("artifacts")}>
            Artifacts<span className="badge">0</span>
          </button>
          <div className="divider" />
          <button
            className="icon-btn"
            title="Replay van dit gesprek"
            disabled={!current}
            onClick={() => setReplay(true)}
          >
            ⟲
          </button>
          <button className="icon-btn" title="Geschiedenis" onClick={() => setShowHistory((v) => !v)}>
            <IcClock size={22} />
          </button>
        </div>
        <button className="new-chat" onClick={() => void newSession()}>
          <IcPlus /> New chat
        </button>
      </header>
      {showHistory && <History onClose={() => setShowHistory(false)} />}
      {replay && current && <Replay sessionId={current} onClose={() => setReplay(false)} />}

      {tab === "conversation" ? (
        <div
          className="messages"
          ref={scroller}
          onScroll={(e) => {
            const el = e.currentTarget;
            stick.current = el.scrollHeight - el.scrollTop - el.clientHeight < 60;
          }}
        >
          {!current || messages.length === 0 ? (
            <div className="empty">
              <div>
                <h2>Waar gaan we aan werken?</h2>
                <p>Antwoorden worden lokaal gegenereerd en bewaard. Stoppen kan altijd met de oranje knop.</p>
              </div>
            </div>
          ) : (
            messages.map((m) =>
              m.role === "user" ? (
                <div className="msg-user" key={m.id}>
                  <IcUser size={22} />
                  <div>{m.content}</div>
                </div>
              ) : m.role === "assistant" && current ? (
                <Assistant key={m.id} m={m} sessionId={current} />
              ) : null,
            )
          )}
        </div>
      ) : (
        <div className="empty">
          {tab === "sources"
            ? "Bronnen verschijnen hier vanaf research (W10)."
            : "Artefacten verschijnen hier vanaf tools (W08)."}
        </div>
      )}

      <TaskBar />
      <Composer
        models={models}
        defaultModel={defaultModel}
        activeRun={activeRun}
        onSend={send}
        onStop={async () => {
          if (activeRun) await api.stop(activeRun);
        }}
        onSteer={async (t) => {
          if (activeRun) await api.steer(activeRun, t);
        }}
      />
    </section>
  );
}
