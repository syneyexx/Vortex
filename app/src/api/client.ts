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
  steer: (runId: string, text: string) =>
    req<{ accepted: boolean }>("POST", `/api/runs/${runId}/steer`, { text }),
  agenda: () =>
    req<
      {
        id: string;
        topic: string;
        reason: string;
        detail: string;
        priority: number;
        status: string;
        hits: number;
      }[]
    >("GET", "/api/agenda"),
  setAgenda: (id: string, status: string) => req<unknown>("POST", `/api/agenda/${id}`, { status }),
  profile: () =>
    req<{
      profile: { style: string[]; facts: string[]; goals: string[]; feedback: Record<string, number> };
      prompt: string | null;
    }>("GET", "/api/profile"),
  tools: () =>
    req<{
      tools: {
        name: string;
        description: string;
        effect: string;
        origin: string;
        core: boolean;
        idempotent: boolean;
      }[];
      mcp: Record<string, string>;
    }>("GET", "/api/tools"),
  reloadTools: () =>
    req<{ skills: number; tools: number; mcp: Record<string, string> }>("POST", "/api/tools/reload", {}),
  skills: () =>
    req<{
      dir: string;
      skills: {
        name: string;
        description: string;
        origin: string;
        version: string;
        globs: string[];
        always_apply: boolean;
        scripts: string[];
        stats: { uses: number; successes: number; failures: number; status: string } | null;
      }[];
    }>("GET", "/api/skills"),
  importSkill: (path: string) => req<{ imported: string[] }>("POST", "/api/skills/import", { path }),
  exportSkill: (name: string, format: string, dir: string) =>
    req<{ path: string }>("POST", `/api/skills/${name}/export`, { format, dir }),
  skillStatus: (name: string, status: string) =>
    req<unknown>("POST", `/api/skills/${name}/status`, { status }),
  deleteSkill: (name: string) => req<unknown>("DELETE", `/api/skills/${name}`),
  proposals: () =>
    req<{ id: string; name: string; description: string; body: string; seen: number }[]>(
      "GET",
      "/api/skills/proposals",
    ),
  decideProposal: (id: string, status: string) =>
    req<{ name: string }>("POST", `/api/skills/proposals/${id}`, { status }),
  teamConflict: (id: string) => req<{ resolution: unknown }>("POST", `/api/memory/conflicts/${id}/team`, {}),
  rollbackConflict: (id: string) =>
    req<{ restored: number }>("POST", `/api/memory/conflicts/${id}/rollback`, {}),
  startResearch: (question: string, mode: string) =>
    req<{ id: string }>("POST", "/api/research", { question, mode }),
  researchList: () =>
    req<{ id: string; question: string; mode: string; status: string; created_at: string }[]>(
      "GET",
      "/api/research",
    ),
  research: (id: string) => req<ResearchDetail>("GET", `/api/research/${id}`),
  corpus: (question: string) => req<{ answer: string }>("POST", "/api/research/corpus", { question }),
  ingest: (paths: string[]) =>
    req<{
      done: boolean;
      report?: {
        docs: number;
        sections: number;
        skipped: number;
        summaries: number;
        errors: string[];
        via_worker: boolean;
      };
    }>("POST", "/api/ingest", { paths }),
  documents: () =>
    req<{ uri: string; title: string | null; pieces: number; updated_at: string }[]>(
      "GET",
      "/api/ingest/documents",
    ),
  neural: () => req<NeuralStatus>("GET", "/api/neural"),
  neuralAction: (layer: string, action: "train" | "bootstrap" | "promote" | "rollback") =>
    req<Record<string, unknown>>("POST", `/api/neural/${layer}/${action}`, {}),
  neuralMode: (layer: string, mode: string) => req<unknown>("POST", `/api/neural/${layer}/mode`, { mode }),
  rerankerDataset: () =>
    req<{ path: string; pairs: number; train: string }>("POST", "/api/neural/reranker/dataset", {}),
};

export interface ResearchDetail {
  id: string;
  question: string;
  mode: string;
  status: string;
  report: string | null;
  report_path: string | null;
  subquestions: {
    i: number;
    text: string;
    status: string;
    answer: string;
    cited: number[];
    next_query: string | null;
  }[];
  sources: {
    n: number;
    kind: string;
    uri: string;
    title: string;
    domain: string;
    locator: string | null;
    reliability: number;
  }[];
  records: {
    id: string;
    kind: string;
    data: Record<string, unknown>;
    source_url: string | null;
    status: string;
    subquestion: number | null;
  }[];
}

export interface NeuralStatus {
  specs: { name: string; description: string; used_in: string; features: string[] }[];
  layers: {
    layer: string;
    manifest: {
      mode: "off" | "shadow" | "active";
      active: string | null;
      shadow: string | null;
      previous: string[];
      versions: {
        version: string;
        created_at: string;
        dataset: string;
        eval: { n: number; net: number; heuristic: number } | null;
      }[];
    };
    shadow_n: number;
    shadow_agree: number;
    decisions: number;
  }[];
  runs: {
    layer: string;
    version: string | null;
    net: number | null;
    heuristic: number | null;
    decision: string | null;
    created_at: string;
  }[];
  examples: Record<string, number>;
  min_examples: number;
}
