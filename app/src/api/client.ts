// REST-client voor de kern. In de browser (geserveerd door `vortex serve`) = zelfde origin;
// in dev/Tauri: VITE_VORTEX_API of standaard http://127.0.0.1:7420.
import type {
  ChatMessage,
  ConflictRow,
  EvalComparison,
  EvalResult,
  EvalRun,
  Memory,
  Mode,
  ModelInfo,
  Retrieved,
  RunRecord,
  Session,
  Status,
  TraceResp,
  VxEvent,
} from "./types.ts";

function defaultBase(): string {
  const env = import.meta.env.VITE_VORTEX_API as string | undefined;
  if (env) return env.replace(/\/$/, "");
  const { protocol, host } = window.location;
  if (protocol.startsWith("http") && !host.endsWith(":1420")) return `${protocol}//${host}`;
  return "http://127.0.0.1:7420";
}

export const API_BASE = defaultBase();

async function req<T>(method: string, path: string, body?: unknown): Promise<T> {
  const r = await fetch(`${API_BASE}${path}`, {
    method,
    headers: body === undefined ? undefined : { "content-type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const data = (await r.json().catch(() => ({}))) as T & { error?: string };
  if (!r.ok) throw new Error(data.error ?? `HTTP ${r.status}`);
  return data;
}

export const api = {
  status: () => req<Status>("GET", "/api/status"),
  sessions: () => req<Session[]>("GET", "/api/sessions"),
  createSession: () => req<Session>("POST", "/api/sessions"),
  session: (id: string) => req<{ session: Session; messages: ChatMessage[] }>("GET", `/api/sessions/${id}`),
  rename: (id: string, title: string) => req<unknown>("PATCH", `/api/sessions/${id}`, { title }),
  archive: (id: string) => req<unknown>("DELETE", `/api/sessions/${id}`),
  fork: (id: string, messageId: string) =>
    req<Session>("POST", `/api/sessions/${id}/fork`, { message_id: messageId }),
  send: (id: string, text: string, mode: Mode, model?: string) =>
    req<{ run_id: string; user_message_id: string; assistant_message_id: string; model: string }>(
      "POST",
      `/api/sessions/${id}/messages`,
      { text, mode, model },
    ),
  stop: (runId: string) => req<{ accepted: boolean }>("POST", `/api/runs/${runId}/stop`),
  pause: (runId: string) => req<{ paused: boolean }>("POST", `/api/runs/${runId}/pause`),
  resume: (runId: string) => req<{ paused: boolean }>("POST", `/api/runs/${runId}/resume`),
  runs: () => req<{ active: RunRecord[]; recent: RunRecord[] }>("GET", "/api/runs"),
  models: () => req<{ default: string; models: ModelInfo[] }>("GET", "/api/models"),
  prompts: () =>
    req<{ version: string; files: { path: string; content: string }[] }>("GET", "/api/settings/prompts"),
  savePrompt: (path: string, content: string) =>
    req<{ version: string }>("PUT", "/api/settings/prompts", { path, content }),
  history: () => req<{ version: string; ts: string }[]>("GET", "/api/settings/history"),
  restore: (version: string, path: string) =>
    req<{ version: string }>("POST", "/api/settings/restore", { version, path }),
  memories: (p: {
    q?: string;
    kind?: string;
    workspace?: string;
    stale?: boolean;
    limit?: number;
    offset?: number;
  }) => {
    const qs = new URLSearchParams();
    Object.entries(p).forEach(([k, v]) => {
      if (v !== undefined && v !== "") qs.set(k, String(v));
    });
    return req<Memory[]>("GET", `/api/memory?${qs.toString()}`);
  },
  memory: (id: string) =>
    req<{ memory: Memory; versions: [number, string, string, string][] }>("GET", `/api/memory/${id}`),
  memoryStats: () => req<Record<string, unknown>>("GET", "/api/memory/stats"),
  remember: (content: string, kind: string, workspace?: string) =>
    req<unknown>("POST", "/api/memory", { content, kind, workspace: workspace || undefined }),
  editMemory: (id: string, content: string) => req<Memory>("PATCH", `/api/memory/${id}`, { content }),
  forget: (id: string) => req<{ forgotten: string[] }>("DELETE", `/api/memory/${id}`),
  searchMemory: (text: string, workspace?: string) =>
    req<Retrieved[]>("POST", "/api/memory/search", { text, workspace: workspace || undefined, k: 10 }),
  conflicts: (status?: string) =>
    req<ConflictRow[]>("GET", `/api/memory/conflicts${status ? `?status=${status}` : ""}`),
  resolveConflict: (id: string, winner: string | null, note: string) =>
    req<unknown>("POST", `/api/memory/conflicts/${id}/resolve`, { winner, note }),
  workspaces: () => req<string[]>("GET", "/api/memory/workspaces"),
  backup: () => req<{ path: string }>("POST", "/api/memory/backup", {}),
  importMemory: async (body: string): Promise<{ imported?: number; error?: string }> => {
    const r = await fetch(`${API_BASE}/api/memory/import`, { method: "POST", body });
    return (await r.json()) as { imported?: number; error?: string };
  },
  exportUrl: () => `${API_BASE}/api/memory/export`,
  feedback: (messageId: string, rating: "good" | "bad" | "correction", correction?: string) =>
    req<{ ok: boolean }>("POST", `/api/messages/${messageId}/feedback`, { rating, correction }),
  trace: (runId: string) => req<TraceResp>("GET", `/api/runs/${runId}/trace`),
  replay: (sessionId: string) => req<{ events: VxEvent[] }>("GET", `/api/sessions/${sessionId}/replay`),
  analytics: () => req<Record<string, unknown>>("GET", "/api/analytics"),
  evalSuites: () => req<{ name: string; tasks: number }[]>("GET", "/api/eval/suites"),
  evalRun: (suite: string, label?: string) =>
    req<{ started: string[] }>("POST", "/api/eval/run", { suite, label }),
  evalRuns: (suite?: string) => req<EvalRun[]>("GET", `/api/eval/runs${suite ? `?suite=${suite}` : ""}`),
  evalRunDetail: (id: string) =>
    req<{ comparison: EvalComparison; results: EvalResult[] }>("GET", `/api/eval/runs/${id}`),
  evalRetrieval: () => req<Record<string, unknown>>("POST", "/api/eval/retrieval", {}),
  evalLeakage: () =>
    req<{ checked_evals: number; checked_memories: number; leaks: unknown[] }>("GET", "/api/eval/leakage"),
};
