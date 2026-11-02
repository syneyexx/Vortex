// Pure reducer: events → GUI-toestand. Getest in reduce.test.ts.
import type { ChatMessage, MessageStatus, RetrievalItem, Session, VxEvent } from "../api/types.ts";

export interface RunInfo {
  id: string;
  kind: string;
  state: string;
  sessionId?: string;
  startedAt: number;
}

export interface Toast {
  id: number;
  text: string;
}

export interface StreamStat {
  startedAt: number;
  chars: number;
}

export interface State {
  sessions: Session[];
  currentSession: string | null;
  messages: Record<string, ChatMessage[]>;
  runs: Record<string, RunInfo>;
  streamStats: Record<string, StreamStat>;
  toasts: Toast[];
  /** Opgehaalde herinneringen per run (Retrieved memory-paneel). */
  retrievals: Record<string, RetrievalItem[]>;
  /** Sessies waarvan de server-versie opnieuw opgehaald moet worden (metingen e.d.). */
  dirty: string[];
  lastSeq: number;
}

export const initialState: State = {
  sessions: [],
  currentSession: null,
  messages: {},
  runs: {},
  streamStats: {},
  retrievals: {},
  toasts: [],
  dirty: [],
  lastSeq: 0,
};

const TERMINAL = new Set(["completed", "failed", "cancelled"]);

function str(v: unknown): string {
  return typeof v === "string" ? v : "";
}

function upsertMsg(list: ChatMessage[], m: ChatMessage): ChatMessage[] {
  if (list.some((x) => x.id === m.id)) return list;
  return [...list, m].sort((a, b) => a.ord - b.ord);
}

let toastId = 1;

export function applyEvents(state: State, events: VxEvent[], now = Date.now()): State {
  let s = state;
  for (const e of events) {
    const p = e.payload;
    const sid = str(p.session_id);
    switch (e.kind) {
      case "chat.session.created":
      case "chat.session.forked":
      case "chat.session.updated":
        s = { ...s, dirty: [...s.dirty, "__sessions"] };
        break;
      case "chat.message.received":
      case "chat.message.started": {
        const list = s.messages[sid];
        if (!list) break; // sessie niet geladen
        const user = e.kind === "chat.message.received";
        const m: ChatMessage = {
          id: str(p.message_id),
          session_id: sid,
          ord: (list.at(-1)?.ord ?? 0) + 1,
          role: user ? "user" : "assistant",
          content: user ? str(p.text) : "",
          reasoning: "",
          tool_calls: [],
          run_id: e.run_id ?? null,
          model: user ? null : str(p.model),
          mode: user ? str(p.mode) : null,
          config_version: str(p.config_version) || null,
          status: user ? "complete" : "streaming",
          metrics: null,
          created_at: e.ts,
        };
        s = { ...s, messages: { ...s.messages, [sid]: upsertMsg(list, m) } };
        if (!user) s = { ...s, streamStats: { ...s.streamStats, [m.id]: { startedAt: now, chars: 0 } } };
        break;
      }
      case "chat.message.delta": {
        const list = s.messages[sid];
        if (!list) break;
        const id = str(p.message_id);
        const text = str(p.text);
        const reasoning = str(p.reasoning);
        s = {
          ...s,
          messages: {
            ...s.messages,
            [sid]: list.map((m) =>
              m.id === id ? { ...m, content: m.content + text, reasoning: m.reasoning + reasoning } : m,
            ),
          },
        };
        const st = s.streamStats[id] ?? { startedAt: now, chars: 0 };
        s = {
          ...s,
          streamStats: {
            ...s.streamStats,
            [id]: { ...st, chars: st.chars + text.length + reasoning.length },
          },
        };
        break;
      }
      case "chat.message.completed": {
        const list = s.messages[sid];
        const id = str(p.message_id);
        const status = (str(p.status) || "complete") as MessageStatus;
        if (list) {
          s = {
            ...s,
            messages: {
              ...s.messages,
              [sid]: list.map((m) =>
                m.id === id ? { ...m, status, error: (p.error as string | null) ?? null } : m,
              ),
            },
          };
        }
        s = { ...s, dirty: [...s.dirty, sid] };
        if (sid !== s.currentSession && status === "complete") {
          s = { ...s, toasts: [...s.toasts, { id: toastId++, text: "Antwoord klaar in een andere chat" }] };
        }
        break;
      }
      case "memory.retrieval.performed":
        if (e.run_id) {
          s = {
            ...s,
            retrievals: { ...s.retrievals, [e.run_id]: (p.items as RetrievalItem[] | undefined) ?? [] },
          };
        }
        break;
      case "eval.run.completed": {
        const score = typeof p.score === "number" ? `${Math.round(p.score * 100)}%` : "—";
        s = { ...s, toasts: [...s.toasts, { id: toastId++, text: `Eval ${str(p.suite)} klaar: ${score}` }] };
        break;
      }
      case "run.lifecycle.created":
        if (e.run_id)
          s = {
            ...s,
            runs: {
              ...s.runs,
              [e.run_id]: { id: e.run_id, kind: str(p.kind), state: "queued", startedAt: now },
            },
          };
        break;
      case "run.state.changed":
      case "run.lifecycle.resumed": {
        if (!e.run_id) break;
        const prev = s.runs[e.run_id] ?? { id: e.run_id, kind: "?", state: "?", startedAt: now };
        const to = str(p.to);
        const rid = e.run_id;
        let runs: Record<string, RunInfo> = { ...s.runs, [rid]: { ...prev, state: to } };
        if (TERMINAL.has(to)) {
          runs = Object.fromEntries(Object.entries(runs).filter(([k]) => k !== rid));
          // notificatie voor achtergrondwerk (geen chat)
          if (prev.kind !== "chat" && prev.kind !== "eval" && prev.kind !== "?") {
            s = {
              ...s,
              toasts: [...s.toasts, { id: toastId++, text: `Achtergrondtaak ${prev.kind}: ${to}` }],
            };
          }
        }
        s = { ...s, runs };
        break;
      }
    }
    s = { ...s, lastSeq: Math.max(s.lastSeq, e.seq) };
  }
  return s;
}
