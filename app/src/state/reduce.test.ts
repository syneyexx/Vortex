import { describe, expect, it } from "vitest";
import type { ChatMessage, VxEvent } from "../api/types.ts";
import { applyEvents, initialState, type State } from "./reduce.ts";

let seq = 0;
const ev = (kind: string, payload: Record<string, unknown>, run_id = "run_1"): VxEvent => ({
  id: `evt_${++seq}`,
  seq,
  kind,
  schema_version: 1,
  ts: new Date().toISOString(),
  run_id,
  trace_id: "trc_1",
  provenance: { source: "system" },
  payload,
});

const loaded = (): State => ({
  ...initialState,
  currentSession: "s1",
  messages: { s1: [] as ChatMessage[] },
});

describe("applyEvents", () => {
  it("bouwt een gestreamd antwoord op uit deltas", () => {
    let s = loaded();
    s = applyEvents(s, [
      ev("chat.message.received", { session_id: "s1", message_id: "u1", text: "hoi" }),
      ev("chat.message.started", { session_id: "s1", message_id: "a1", model: "m" }),
      ev("chat.message.delta", { session_id: "s1", message_id: "a1", text: "Hal", reasoning: "" }),
      ev("chat.message.delta", { session_id: "s1", message_id: "a1", text: "lo", reasoning: "denk" }),
    ]);
    const msgs = s.messages.s1 ?? [];
    expect(msgs.map((m) => m.role)).toEqual(["user", "assistant"]);
    expect(msgs[1]?.content).toBe("Hallo");
    expect(msgs[1]?.reasoning).toBe("denk");
    expect(msgs[1]?.status).toBe("streaming");
    s = applyEvents(s, [
      ev("chat.message.completed", { session_id: "s1", message_id: "a1", status: "cancelled" }),
    ]);
    expect(s.messages.s1?.[1]?.status).toBe("cancelled");
    expect(s.dirty).toContain("s1");
  });

  it("dubbele events (na reconnect) voegen niets dubbel toe", () => {
    const start = ev("chat.message.started", { session_id: "s1", message_id: "a2" });
    let s = applyEvents(loaded(), [start]);
    s = applyEvents(s, [start]);
    expect(s.messages.s1?.length).toBe(1);
  });

  it("negeert events van niet-geladen sessies maar houdt seq bij", () => {
    const e = ev("chat.message.delta", { session_id: "anders", message_id: "x", text: "a" });
    const s = applyEvents(loaded(), [e]);
    expect(s.messages.anders).toBeUndefined();
    expect(s.lastSeq).toBe(e.seq);
  });

  it("houdt actieve runs bij en meldt afgeronde achtergrondtaken", () => {
    let s = applyEvents(loaded(), [ev("run.lifecycle.created", { kind: "ingest" }, "r9")]);
    expect(Object.keys(s.runs)).toEqual(["r9"]);
    s = applyEvents(s, [ev("run.state.changed", { from: "running", to: "completed" }, "r9")]);
    expect(Object.keys(s.runs)).toEqual([]);
    expect(s.toasts.at(-1)?.text).toContain("ingest");
  });
});
