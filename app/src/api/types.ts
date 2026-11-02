// Typen van de kern-API (spiegel van vortex-server / vortex-types). Event = één frame op de stream.

export interface VxEvent {
  id: string;
  seq: number;
  kind: string;
  schema_version: number;
  ts: string;
  run_id?: string;
  trace_id: string;
  span_id?: string;
  provenance: { source: string; origin?: string };
  payload: Record<string, unknown>;
}

export interface Session {
  id: string;
  title: string;
  parent_session_id: string | null;
  forked_from_message: string | null;
  created_at: string;
  updated_at: string;
  message_count: number;
}

export type MessageStatus = "complete" | "streaming" | "failed" | "cancelled";

export interface ChatMessage {
  id: string;
  session_id: string;
  ord: number;
  role: "user" | "assistant" | "system" | "tool";
  content: string;
  reasoning: string;
  tool_calls: unknown[];
  run_id: string | null;
  model: string | null;
  mode: string | null;
  config_version: string | null;
  status: MessageStatus;
  metrics: { metrics?: { decode_tps?: number; ttft_ms?: number }; finish_reason?: string } | null;
  created_at: string;
  error?: string | null;
}

export interface Gpu {
  index: number;
  name: string;
  vram_total_mib: number;
  vram_used_mib: number;
}

export interface Status {
  version: string;
  subsystems: { name: string; ok: boolean; detail: string }[];
  gpus: Gpu[];
  gpu_detail: string | null;
  servers: {
    name: string;
    url: string;
    role: string;
    model: string | null;
    ready: boolean;
    detail: string;
  }[];
  default_model: string | null;
  tok_s: number[];
  last_seq: number;
  active_runs: number;
}

export interface RunRecord {
  id: string;
  kind: string;
  priority: string;
  state: string;
  created_at: string;
  updated_at: string;
  error: string | null;
  usage: { steps: number; tokens: number; elapsed_ms: number };
  input: Record<string, unknown>;
}

export interface ModelInfo {
  name: string;
  roles: string[];
  server: string;
  file: string;
  context: number | null;
  kv_type: string | null;
  parser: string;
  loaded: boolean;
  chat: boolean;
}

export type Mode = "adaptive" | "quick" | "deep" | "research";

export interface Memory {
  id: string;
  kind: string;
  workspace: string | null;
  session_id: string | null;
  title: string;
  content: string;
  context: string;
  summary: string;
  source_uri: string | null;
  locator: string | null;
  provenance: { source: string; origin?: string };
  importance: number;
  salience: number;
  confidence: number;
  valid_to: string | null;
  access_count: number;
  version: number;
  stale: boolean;
  merged_count: number;
  tags: string[];
  created_at: string;
  updated_at: string;
}

export interface ScoreParts {
  vector: number | null;
  bm25_rank: number | null;
  graph_hops: number | null;
  temporal: boolean;
  rrf: number;
  rerank: number | null;
  recency: number;
  importance: number;
  final_score: number;
}

export interface Retrieved {
  memory: Memory;
  score: ScoreParts;
  counter_evidence: boolean;
}

export interface RetrievalItem {
  id: string;
  kind: string;
  score: ScoreParts;
  counter: boolean;
}

export interface ConflictRow {
  conflict: {
    id: string;
    a: string;
    b: string;
    reason: string;
    status: string;
    resolution: string | null;
    created_at: string;
  };
  a: Memory | null;
  b: Memory | null;
}

export interface EvalRun {
  id: string;
  suite: string;
  version: string;
  label: string | null;
  started_at: string;
  finished_at: string | null;
  n: number;
  passed: number;
  skipped: number;
  score: number | null;
}

export interface EvalResult {
  task_id: string;
  status: string;
  score: number;
  latency_ms: number;
  output: string;
  detail: string;
}

export interface EvalComparison {
  suite: string;
  current: EvalRun;
  previous: EvalRun | null;
  delta: number | null;
  regressions: string[];
  improvements: string[];
  verdict: string;
}

export interface TraceResp {
  run: RunRecord;
  spans: { wait_ms: number | null; execute_ms: number | null; total_ms: number };
  inference: Record<string, number | null>[];
  events: { seq: number; t_ms: number; kind: string; payload: Record<string, unknown> }[];
}
