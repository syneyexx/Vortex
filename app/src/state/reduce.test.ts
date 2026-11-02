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

  it("volgt het debatteam per run", () => {
    const s = applyEvents(loaded(), [
      ev("agent.team.composed", { roles: ["Planner", "Solvers ×2", "Judge", "Synthesizer"] }, "r1"),
      ev("agent.role.started", { role: "Planner", detail: "deelvragen" }, "r1"),
      ev("agent.role.completed", { role: "Planner", summary: "3 stappen" }, "r1"),
      ev("agent.judge.verdict", { verdict: { verdict: "accept", confidence: 0.8, reasons: ["ok"] } }, "r1"),
      ev(
        "agent.evidence.compiled",
        {
          checks: [{ claim: "x", label: "supported", why: "", scope: "lokaal", sources: [] }],
          scope: "lokaal",
        },
        "r1",
      ),
    ]);
    const c = s.cognition.r1;
    expect(c?.team?.length).toBe(4);
    expect(c?.roles?.Planner?.summary).toBe("3 stappen");
    expect(c?.judge?.verdict).toBe("accept");
    expect(c?.evidence?.checks[0]?.label).toBe("supported");
  });

  it("bouwt tool-kaarten per run", () => {
    const s = applyEvents(loaded(), [
      ev("tool.call.started", { action_id: "a1", tool: "write_file", input: { path: "x" } }, "r1"),
      ev("tool.call.started", { action_id: "a2", tool: "read_file" }, "r1"),
      ev("tool.call.completed", { action_id: "a1", tool: "write_file", ms: 4, preview: "+x" }, "r1"),
      ev(
        "tool.call.failed",
        { action_id: "a2", tool: "read_file", error: { message: "bestaat niet" } },
        "r1",
      ),
    ]);
    const t = s.tools.r1 ?? [];
    expect(t.map((x) => x.status)).toEqual(["ok", "error"]);
    expect(t[0]?.preview).toBe("+x");
    expect(t[1]?.error).toBe("bestaat niet");
  });

  it("volgt de cognitieve cyclus per run", () => {
    const s = applyEvents(loaded(), [
      ev(
        "cognition.strategy.chosen",
        { path: "deliberate", plan: "plan_execute", prediction: "answer", why: ["x"] },
        "r1",
      ),
      ev("cognition.plan.created", { steps: ["a", "b"] }, "r1"),
      ev("cognition.plan.step_completed", { i: 0 }, "r1"),
      ev("cognition.phase.completed", { phase: "perceive", label: "Waarnemen", ms: 3 }, "r1"),
      ev("cognition.verify.completed", { ok: false, issues: ["leeg"] }, "r1"),
    ]);
    const c = s.cognition.r1;
    expect(c?.strategy?.path).toBe("deliberate");
    expect(c?.steps).toEqual(["a", "b"]);
    expect(c?.stepsDone).toBe(1);
    expect(c?.phases[0]?.label).toBe("Waarnemen");
    expect(c?.verify?.ok).toBe(false);
  });

  it("onthoudt opgehaalde herinneringen per run", () => {
    const s = applyEvents(loaded(), [
      ev(
        "memory.retrieval.performed",
        { items: [{ id: "mem_1", kind: "user", score: { final_score: 0.8 }, counter: false }] },
        "r1",
      ),
    ]);
    expect(s.retrievals.r1?.[0]?.id).toBe("mem_1");
  });

  it("eval-taken geven geen meldingsvloed, wel één samenvatting", () => {
    let s = applyEvents(loaded(), [ev("run.lifecycle.created", { kind: "eval" }, "e1")]);
    s = applyEvents(s, [ev("run.state.changed", { to: "failed" }, "e1")]);
    expect(s.toasts.length).toBe(0);
    s = applyEvents(s, [ev("eval.run.completed", { suite: "redeneren", score: 0.75 })]);
    expect(s.toasts.at(-1)?.text).toBe("Eval redeneren klaar: 75%");
  });

  it("houdt actieve runs bij en meldt afgeronde achtergrondtaken", () => {
    let s = applyEvents(loaded(), [ev("run.lifecycle.created", { kind: "ingest" }, "r9")]);
    expect(Object.keys(s.runs)).toEqual(["r9"]);
    s = applyEvents(s, [ev("run.state.changed", { from: "running", to: "completed" }, "r9")]);
    expect(Object.keys(s.runs)).toEqual([]);
    expect(s.toasts.at(-1)?.text).toContain("ingest");
  });

  it("meldt autonomie-gebeurtenissen (briefing, suggestie, GPU, mislukte backup)", () => {
    let s = loaded();
    s = applyEvents(s, [
      ev("autonomy.briefing.written", { day: "2026-11-02" }),
      ev("autonomy.suggestion.created", { text: "Nu een backup maken?" }),
      ev("autonomy.gpu.released", {}),
      ev("autonomy.backup.completed", { verified: true }),
      ev("autonomy.backup.completed", { verified: false }),
    ]);
    const texts = s.toasts.map((t) => t.text);
    expect(texts).toContain("Briefing 2026-11-02 staat klaar");
    expect(texts).toContain("Suggestie: Nu een backup maken?");
    expect(texts.filter((t) => t.startsWith("Backup"))).toHaveLength(1);
  });
});
