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
