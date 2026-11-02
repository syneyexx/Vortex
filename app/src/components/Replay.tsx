// Session replay uit het event log (VX:W06.08): schuif door de tijd en zie het gesprek ontstaan.
import { useEffect, useMemo, useState } from "react";
import { api } from "../api/client.ts";
import type { ChatMessage, VxEvent } from "../api/types.ts";
import { applyEvents, initialState } from "../state/reduce.ts";
import { setError } from "../state/store.ts";

export function Replay({ sessionId, onClose }: { sessionId: string; onClose: () => void }) {
  const [events, setEvents] = useState<VxEvent[]>([]);
  const [pos, setPos] = useState(0);
  const [playing, setPlaying] = useState(false);
  useEffect(() => {
    api
      .replay(sessionId)
      .then((r) => {
        setEvents(r.events);
        setPos(r.events.length);
      })
      .catch((e: unknown) => setError(String(e)));
  }, [sessionId]);
  useEffect(() => {
    if (!playing) return;
    const t = setInterval(() => setPos((p) => (p >= events.length ? (setPlaying(false), p) : p + 1)), 60);
    return () => clearInterval(t);
  }, [playing, events.length]);
  const msgs: ChatMessage[] = useMemo(() => {
    const s = applyEvents({ ...initialState, messages: { [sessionId]: [] } }, events.slice(0, pos));
    return s.messages[sessionId] ?? [];
  }, [events, pos, sessionId]);
  const cur = events[pos - 1];
  return (
    <div className="modal-back" onClick={onClose}>
      <div className="panel modal" onClick={(e) => e.stopPropagation()}>
        <div className="row">
          <h2 style={{ margin: 0, fontWeight: 500 }}>Replay</h2>
          <button
            className="btn"
            onClick={() => {
              setPos(0);
              setPlaying(true);
            }}
          >
            ▶ Afspelen
          </button>
          <button className="btn" onClick={() => setPlaying(false)}>
            ❚❚
          </button>
          <span style={{ color: "var(--vx-muted)", fontSize: 13 }}>
            {pos}/{events.length} ·{" "}
            {cur ? `${cur.kind} · ${new Date(cur.ts).toLocaleTimeString("nl-NL")}` : "begin"}
          </span>
          <button className="btn" style={{ marginLeft: "auto" }} onClick={onClose}>
            Sluiten
          </button>
        </div>
        <input
          type="range"
          min={0}
          max={events.length}
          value={pos}
          onChange={(e) => setPos(Number(e.target.value))}
          style={{ width: "100%", margin: "12px 0" }}
        />
        <div style={{ maxHeight: "60vh", overflow: "auto", display: "grid", gap: 10 }}>
          {msgs.map((m) => (
            <div
              key={m.id}
              className={m.role === "user" ? "msg-user" : "prose"}
              style={m.role === "user" ? { alignSelf: "end" } : undefined}
            >
              {m.content || <em style={{ color: "var(--vx-muted)" }}>…</em>}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
