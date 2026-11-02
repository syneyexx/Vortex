// Pure reducer: events → GUI-toestand. Getest in reduce.test.ts.
import type { ChatMessage, MessageStatus, RetrievalItem, Session, VxEvent } from "../api/types.ts";

export interface RunInfo {
  id: string;
  kind: string;
  state: string;
  sessionId?: string;
  startedAt: number;
}

export interface RoleStatus {
  status: "busy" | "done";
  summary: string;
}

export interface EvidenceItem {
  claim: string;
  label: "supported" | "contradicted" | "unverified";
  why: string;
  scope: string;
  sources: { kind: string; reference: string; date?: string | null; snippet: string }[];
}

export interface Cognition {
  phases: { phase: string; label: string; ms: number }[];
  /** Debatteam (W09). */
  team?: string[];
  roles?: Record<string, RoleStatus>;
  judge?: { verdict: string; confidence: number; reasons: string[] };
  evidence?: { checks: EvidenceItem[]; scope: string };
  strategy?: { path: string; plan: string; prediction: string; why: string[] };
  steps: string[];
  stepsDone: number;
  verify?: { ok: boolean; issues: string[] };
}

export interface ToolCard {
  id: string;
  tool: string;
  input?: unknown;
  status: "running" | "ok" | "error";
  ms?: number;
  preview?: string;
  output?: string;
  error?: string;
  replayed?: boolean;
  cached?: boolean;
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
  /** Toolaanroepen per run (tool-kaarten in de chat). */
  tools: Record<string, ToolCard[]>;
  /** Cognitieve cyclus per run (Agent collaboration-paneel). */
  cognition: Record<string, Cognition>;
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
  tools: {},
  cognition: {},
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
      case "cognition.phase.completed":
      case "cognition.strategy.chosen":
      case "cognition.plan.created":
      case "cognition.plan.step_completed":
      case "cognition.verify.completed":
      case "agent.team.composed":
      case "agent.role.started":
      case "agent.role.completed":
      case "agent.judge.verdict":
      case "agent.evidence.compiled": {
        if (!e.run_id) break;
        const c: Cognition = s.cognition[e.run_id] ?? { phases: [], steps: [], stepsDone: 0 };
        let n: Cognition = c;
        if (e.kind === "cognition.phase.completed")
          n = {
            ...c,
            phases: [...c.phases, { phase: str(p.phase), label: str(p.label), ms: Number(p.ms ?? 0) }],
          };
        else if (e.kind === "cognition.strategy.chosen")
          n = {
            ...c,
            strategy: {
              path: str(p.path),
              plan: str(p.plan),
              prediction: str(p.prediction),
              why: (p.why as string[] | undefined) ?? [],
            },
          };
        else if (e.kind === "cognition.plan.created")
          n = { ...c, steps: (p.steps as string[] | undefined) ?? [] };
        else if (e.kind === "cognition.plan.step_completed") n = { ...c, stepsDone: c.stepsDone + 1 };
        else if (e.kind === "agent.team.composed")
          n = { ...c, team: (p.roles as string[] | undefined) ?? [], roles: {} };
        else if (e.kind === "agent.role.started" || e.kind === "agent.role.completed") {
          const role = str(p.role);
          const st: RoleStatus =
            e.kind === "agent.role.started"
              ? { status: "busy", summary: str(p.detail) }
              : { status: "done", summary: str(p.summary) };
          n = { ...c, roles: { ...(c.roles ?? {}), [role]: st } };
        } else if (e.kind === "agent.judge.verdict") {
          const v = (p.verdict ?? {}) as { verdict?: string; confidence?: number; reasons?: string[] };
          n = {
            ...c,
            judge: {
              verdict: v.verdict ?? "",
              confidence: Number(v.confidence ?? 0),
              reasons: v.reasons ?? [],
            },
          };
        } else if (e.kind === "agent.evidence.compiled")
          n = {
            ...c,
            evidence: { checks: (p.checks as EvidenceItem[] | undefined) ?? [], scope: str(p.scope) },
          };
        else n = { ...c, verify: { ok: Boolean(p.ok), issues: (p.issues as string[] | undefined) ?? [] } };
        s = { ...s, cognition: { ...s.cognition, [e.run_id]: n } };
        break;
      }
      case "tool.call.started":
      case "tool.call.completed":
      case "tool.call.failed":
      case "tool.call.invalid": {
        if (!e.run_id) break;
        const list = s.tools[e.run_id] ?? [];
        const id = str(p.action_id);
        const i = list.findIndex((t) => t.id === id);
        const prev: ToolCard = list[i] ?? { id, tool: str(p.tool), status: "running" };
        let card: ToolCard = prev;
        if (e.kind === "tool.call.started") card = { ...prev, input: p.input, status: "running" };
        else if (e.kind === "tool.call.completed")
          card = {
            ...prev,
            status: "ok",
            ms: Number(p.ms ?? 0),
            preview: (p.preview as string | null | undefined) ?? undefined,
            output: (p.output as string | undefined) ?? undefined,
            replayed: Boolean(p.replayed),
            cached: Boolean(p.cached),
          };
        else {
          const err = p.error as { message?: string } | undefined;
          card = { ...prev, status: "error", error: err?.message ?? "fout" };
        }
        const next = i >= 0 ? list.map((t, k) => (k === i ? card : t)) : [...list, card];
        s = { ...s, tools: { ...s.tools, [e.run_id]: next } };
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
      // Autonomie (W15): briefing, suggesties, GPU vrijgegeven/terug, backups → melding.
      case "autonomy.briefing.written":
        s = { ...s, toasts: [...s.toasts, { id: toastId++, text: `Briefing ${str(p.day)} staat klaar` }] };
        break;
      case "autonomy.suggestion.created":
        s = { ...s, toasts: [...s.toasts, { id: toastId++, text: `Suggestie: ${str(p.text)}` }] };
        break;
      case "autonomy.gpu.released":
        s = {
          ...s,
          toasts: [...s.toasts, { id: toastId++, text: "GPU vrijgegeven (ander programma actief)" }],
        };
        break;
      case "autonomy.gpu.restored":
        s = { ...s, toasts: [...s.toasts, { id: toastId++, text: "Modellen weer geladen" }] };
        break;
      case "autonomy.backup.completed":
        if (p.verified !== true)
          s = { ...s, toasts: [...s.toasts, { id: toastId++, text: "Backup: restore-test MISLUKT" }] };
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
