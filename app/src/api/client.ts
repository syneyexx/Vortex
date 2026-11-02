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
  send: (id: string, text: string, mode: Mode, model?: string, attachments: string[] = []) =>
    req<{ run_id: string; user_message_id: string; assistant_message_id: string; model: string }>(
      "POST",
      `/api/sessions/${id}/messages`,
      { text, mode, model, attachments },
    ),
  // Multimodaal (W16)
  upload: (name: string, data: string) =>
    req<{ path: string; kind: "image" | "audio" | "video" | "document"; bytes: number }>(
      "POST",
      "/api/uploads",
      { name, data },
    ),
  transcribe: (name: string, data: string) =>
    req<{ text: string; segments: { start: number; end: number; text: string }[] }>(
      "POST",
      "/api/media/transcribe",
      { name, data },
    ),
  speak: (text: string) =>
    req<{ parts: { text: string; url: string; seconds: number }[] }>("POST", "/api/media/speak", { text }),
  imagine: (prompt: string, init_image?: string) =>
    req<{ url: string; engine: string }>("POST", "/api/media/imagine", { prompt, init_image }),
  renderPage: (path: string, page: number) =>
    req<{ url: string; width: number; height: number; zoom: number; page_size: [number, number] }>(
      "POST",
      "/api/media/page",
      { path, page },
    ),
  analyze: (path: string, question = "") =>
    req<{ text: string; result: Record<string, unknown> }>("POST", "/api/media/analyze", { path, question }),
  media: () =>
    req<{
      vision: boolean;
      ocr_engine: string;
      stt_engine: string;
      tts_engine: string;
      imagegen_engine: string;
    }>("GET", "/api/media"),
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
  facts: (entity: string, at?: string) =>
    req<Fact[]>(
      "GET",
      `/api/memory/graph?entity=${encodeURIComponent(entity)}${at ? `&at=${encodeURIComponent(at)}` : ""}`,
    ),
  factHistory: (entity: string) =>
    req<Fact[]>("GET", `/api/memory/graph/history?entity=${encodeURIComponent(entity)}`),
  reason: () =>
    req<{ derived: number; conflicts_found: number; by_rule: Record<string, number> }>(
      "POST",
      "/api/memory/reason",
      {},
    ),
  dream: () => req<Record<string, unknown>>("POST", "/api/memory/dream", {}),
  dreams: () => req<Record<string, unknown>[]>("GET", "/api/memory/dreams"),
  meta: () => req<MetaReport>("GET", "/api/memory/meta"),
  tier: () =>
    req<{ hot: number; cold: number; blocks: number; cold_bytes: number; cached_blocks: number }>(
      "GET",
      "/api/memory/tier",
    ),
  demote: () => req<{ demoted: number }>("POST", "/api/memory/tier/demote", {}),
  addMedia: (path: string, description: string) =>
    req<unknown>("POST", "/api/memory/media", { path, description }),
  selfModel: () => req<{ by_task: Record<string, { score: number; n: number }> }>("GET", "/api/selfmodel"),
  beliefExperiment: () =>
    req<{
      scenarios: {
        name: string;
        n: number;
        bayes_acc: number;
        quantum_acc: number;
        bayes_brier: number;
        quantum_brier: number;
        order_effect: number;
      }[];
      verdict: string;
      theta: number[];
      lambda: number;
    }>("GET", "/api/memory/belief-experiment?n=400"),
  training: () => req<TrainingStatus>("GET", "/api/training"),
  buildDataset: (kind: string) =>
    req<{ version: string; n: number; decontaminated: number }>("POST", `/api/training/datasets/${kind}`, {}),
  trainAdapter: (kind: string, dataset: string) =>
    req<Record<string, unknown>>("POST", "/api/training/train", { kind, dataset }),
  gateAdapter: (id: string) =>
    req<{ status: string; gains: number; regressions: unknown[] }>(
      "POST",
      `/api/training/adapters/${id}/gate`,
      {},
    ),
  shadowAdapter: (id: string) =>
    req<{ n: number; wins: number; win_rate: number }>("POST", `/api/training/adapters/${id}/shadow`, {}),
  scaleAdapter: (id: string, scale: number) =>
    req<{ status: string }>("POST", `/api/training/adapters/${id}/scale`, { scale }),
  calibrate: () => req<Record<string, unknown>>("POST", "/api/training/calibrate", {}),
  // Autonomie (W15)
  autonomy: () => req<AutonomyOverview>("GET", "/api/autonomy"),
  tasks: (status?: string) => req<VxTask[]>("GET", `/api/autonomy/tasks${status ? `?status=${status}` : ""}`),
  addTask: (t: NewVxTask) => req<VxTask>("POST", "/api/autonomy/tasks", t),
  taskAction: (id: string, action: "cancel" | "pause" | "resume") =>
    req<{ ok: boolean }>("POST", `/api/autonomy/tasks/${id}/${action}`, {}),
  projects: () => req<VxProject[]>("GET", "/api/autonomy/projects"),
  addProject: (name: string, goal: string, workspace?: string) =>
    req<VxProject>("POST", "/api/autonomy/projects", { name, goal, workspace }),
  updateProject: (id: string, b: { note?: string; status?: string }) =>
    req<VxProject>("POST", `/api/autonomy/projects/${id}`, b),
  triggers: () => req<VxTrigger[]>("GET", "/api/autonomy/triggers"),
  addTrigger: (b: { name: string; kind: string; spec: string; task: NewVxTask }) =>
    req<{ id: string }>("POST", "/api/autonomy/triggers", b),
  updateTrigger: (id: string, b: { enabled?: boolean; remove?: boolean }) =>
    req<{ ok: boolean }>("POST", `/api/autonomy/triggers/${id}`, b),
  suggestions: (status?: string) =>
    req<VxSuggestion[]>("GET", `/api/autonomy/suggestions${status ? `?status=${status}` : ""}`),
  decideSuggestion: (id: string, accept: boolean) =>
    req<Record<string, unknown>>(
      "POST",
      `/api/autonomy/suggestions/${id}/${accept ? "accept" : "dismiss"}`,
      {},
    ),
  scanSuggestions: () => req<{ new: VxSuggestion[] }>("POST", "/api/autonomy/suggestions/scan", {}),
  briefings: () =>
    req<{ day: string; markdown: string; created_at: string }[]>("GET", "/api/autonomy/briefings"),
  briefingNow: () => req<{ day: string; markdown: string }>("POST", "/api/autonomy/briefings", {}),
  backups: () => req<{ dir: string; backups: VxBackup[] }>("GET", "/api/autonomy/backups"),
  backupNow: () => req<Record<string, unknown>>("POST", "/api/autonomy/backups", {}),
  gpu: () => req<VxGpu>("GET", "/api/autonomy/gpu"),
  progress: () => req<VxProgress>("GET", "/api/autonomy/progress"),
  // Wereldkennis (W14)
  world: () => req<WorldStatus>("GET", "/api/world"),
  worldFetch: (source?: string) => req<Record<string, unknown>>("POST", "/api/world/fetch", { source }),
  worldEvents: (q: { status?: string; hours?: number; theme?: string; region?: string }) =>
    req<WorldEvent[]>(
      "GET",
      `/api/world/events?${new URLSearchParams(
        Object.entries(q)
          .filter(([, v]) => v !== undefined && v !== "")
          .map(([k, v]) => [k, String(v)]),
      ).toString()}`,
    ),
  worldEvent: (id: string) => req<WorldEventDetail>("GET", `/api/world/events/${id}`),
  worldBias: () => req<WorldBias[]>("GET", "/api/world/bias"),
  hypotheses: () => req<Hypothesis[]>("GET", "/api/world/hypotheses"),
  predictions: (status?: string) =>
    req<Prediction[]>("GET", `/api/world/predictions${status ? `?status=${status}` : ""}`),
  addPrediction: (p: { claim: string; probability: number; criteria: string; resolve_by: string }) =>
    req<Prediction>("POST", "/api/world/predictions", p),
  resolvePredictions: () => req<Prediction[]>("POST", "/api/world/predictions/resolve", {}),
  worldCalibration: () => req<WorldCalibration>("GET", "/api/world/calibration"),
  worldSummaries: (period: string, group_by: string) =>
    req<WorldSummary[]>("POST", "/api/world/summaries", { period, group_by }),
  players: () => req<Player[]>("GET", "/api/world/players"),
  addPlayer: (p: { name: string; sector: string; kind: string; public_role: string }) =>
    req<Player>("POST", "/api/world/players", p),
  studyPlayer: (id: string) => req<Record<string, unknown>[]>("POST", `/api/world/players/${id}/study`, {}),
  predictPlayer: (id: string, situation: string, horizon_days: number) =>
    req<Prediction>("POST", `/api/world/players/${id}/predict`, { situation, horizon_days }),
  follow: (url: string, interval_hours: number, remove = false) =>
    req<Record<string, unknown>>("POST", "/api/world/follow", { url, interval_hours, remove }),
  hfCatalog: () => req<HfCatalogRow[]>("GET", "/api/world/datasets"),
  hfInspect: (source: string, license?: string) =>
    req<HfInspect>("POST", "/api/world/datasets/inspect", { source, license: license || undefined }),
  hfImport: (b: {
    source: string;
    destination: string;
    mapping: Record<string, string>;
    kind?: string;
    license?: string;
    limit?: number;
  }) => req<{ id: string; stats: Record<string, unknown> }>("POST", "/api/world/datasets/import", b),
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

export interface Fact {
  id: string;
  src: string;
  rel: string;
  dst: string;
  valid_from: string | null;
  valid_to: string | null;
  confidence: number;
  derived: boolean;
  rule: string | null;
}

export interface MetaReport {
  total: number;
  by_kind: Record<string, { count: number; avg_confidence: number }>;
  stale: number;
  cold: number;
  open_conflicts: number;
  disputed: number;
  derived_facts: number;
  strongest: { name: string; facts: number; mentions: number; avg_confidence: number }[];
  gaps: { name: string; facts: number; mentions: number; avg_confidence: number }[];
}

export interface TrainingStatus {
  datasets: {
    version: string;
    kind: string;
    n: number;
    decontaminated: number;
    stats: Record<string, number>;
    created_at: string;
  }[];
  adapters: {
    id: string;
    kind: string;
    dataset: string;
    status: string;
    scores: Record<string, { adapter: number; base: number }>;
    regressions: { suite: string; base: number; adapter: number }[];
    shadow: { n?: number; win_rate?: number };
    metrics: Record<string, unknown>;
    created_at: string;
  }[];
  calibrator: {
    test_raw: { ece: number; brier: number; n: number };
    test_calibrated: { ece: number; brier: number; n: number };
  } | null;
  config: { base_model: string; idle: boolean; min_new_examples: number };
}

export interface WorldSource {
  name: string;
  kind: string;
  url: string;
  enabled: boolean;
  interval_min: number;
  country?: string;
  orientation?: string;
  terms: string;
  min_gap_s: number;
  stats: { last_ok?: string; items?: number; errors?: number; last_error?: string | null };
}

export interface WorldStatus {
  enabled: boolean;
  sources: WorldSource[];
  counts?: { items: number; events: number; confirmed: number; open_predictions: number; hypotheses: number };
  steps: { name: string; last_ok: string }[];
  followed: { url: string; interval_hours: number; last_fetch?: string; changes: number }[];
}

export interface WorldEvent {
  id: string;
  title: string;
  summary: string;
  first_seen: string;
  last_seen: string;
  status: "reported" | "confirmed";
  n_items: number;
  region?: string;
  theme?: string;
  memory_id?: string;
  entities?: { name?: string; qid?: string; label?: string; description?: string }[];
}

export interface WorldEventDetail extends WorldEvent {
  groups: string[];
  items: { source: string; title: string; url?: string; published?: string; country?: string }[];
}

export interface WorldBias {
  source: string;
  items: number;
  confirmed_share: number;
  regions: number;
  orientation?: string;
  home?: string;
}

export interface Hypothesis {
  id: string;
  cause: string;
  effect: string;
  confidence: number;
  support: number;
  contra: number;
  last_seen: string;
}

export interface Prediction {
  id: string;
  claim: string;
  probability: number;
  criteria: string;
  resolve_by: string;
  actor?: string;
  origin: string;
  status: "open" | "true" | "false" | "unresolvable";
  evidence?: string;
  created_at: string;
  resolved_at?: string;
}

export interface WorldCalibration {
  overall: { n: number; ece: number; brier: number; accuracy: number };
  by: Record<string, { n: number; ece: number; brier: number; accuracy: number }>;
  counts: Record<string, number>;
}

export interface WorldSummary {
  key: string;
  period: string;
  group_by: string;
  events: number;
  confirmed: number;
  summary: string;
}

export interface Player {
  id: string;
  name: string;
  sector: string;
  kind: string;
  sources: string[];
  notes: string;
}

export interface HfInspect {
  schema: { kind: string; fields: Record<string, string> };
  mapping: Record<string, string>;
  lm_mapping?: Record<string, string>;
  lm_kind?: string;
  lm_why?: string;
  sample: Record<string, string>[];
  license?: string | null;
  destinations: string[];
}

export interface HfCatalogRow {
  id: string;
  source: string;
  schema: string;
  destination: string;
  license?: string;
  rows: number;
  kept: number;
  deduped: number;
  decontaminated: number;
  version: string;
  created_at: string;
}

export interface AutonomyOverview {
  user_idle: boolean;
  external_gpu_load: boolean;
  running: VxTask[];
  queued: number;
  suggestions: number;
  last_briefing?: string | null;
  remote: { tailscale: boolean; ip?: string | null; url?: string | null };
}

export interface NewVxTask {
  title: string;
  kind: string;
  params?: Record<string, unknown>;
  priority?: number;
  depends_on?: string[];
  project_id?: string;
  needs_idle?: boolean;
  not_before?: string;
}

export interface VxTask {
  id: string;
  title: string;
  kind: string;
  params: Record<string, unknown>;
  priority: number;
  status: "queued" | "running" | "done" | "failed" | "cancelled" | "paused";
  depends_on: string[];
  project_id?: string | null;
  needs_idle: boolean;
  not_before?: string | null;
  attempts: number;
  max_attempts: number;
  progress: number;
  result?: Record<string, unknown> | null;
  error?: string | null;
  origin: string;
  created_at: string;
  updated_at: string;
}

export interface VxProject {
  id: string;
  name: string;
  goal: string;
  status: string;
  workspace?: string | null;
  notes: string;
  progress: number;
  tasks: VxTask[];
}

export interface VxTrigger {
  id: string;
  name: string;
  kind: "cron" | "interval" | "event";
  spec: string;
  task: NewVxTask;
  enabled: boolean;
  last_fired?: string | null;
  fired: number;
  builtin: boolean;
  next?: string | null;
}

export interface VxSuggestion {
  id: string;
  kind: string;
  text: string;
  action?: NewVxTask | null;
  status: string;
  created_at: string;
}

export interface VxBackup {
  id: string;
  path: string;
  bytes: number;
  tier: string;
  verified: boolean;
  verify: { integrity?: string; error?: string };
  created_at: string;
}

export interface VxGpu {
  enabled: boolean;
  released: boolean;
  restore_after_secs: number;
  servers: { server: string; home: string; phase: string; model?: string | null; inflight: number }[];
  conditions: { user_idle: boolean; external_gpu_load: boolean; pressure: number };
}

export interface VxProgress {
  eval_by_suite: Record<string, Record<string, number>>;
  eval_overall: { month: string; score: number }[];
  trend: { suite: string; last: number; previous: number; delta: number }[];
  memory_by_month: Record<string, Record<string, number>>;
  layers_promoted: number;
  adapters_active: number;
  agenda_done: number;
  agenda_open: number;
  tasks_done: number;
  world_calibration: { n: number; brier: number; ece: number };
}
