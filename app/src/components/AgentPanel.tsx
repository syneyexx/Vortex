// Agent collaboration + Verdict: live de cognitieve cyclus van het laatste antwoord (fasen,
// strategie, planstappen, verificatie). Het debatteam (Planner/Solvers/Critic/Judge) vult dit
// verder vanaf W09. VX:W07.01
import { useStore } from "../state/store.ts";
import type { Cognition } from "../state/reduce.ts";
import { IcCheckCircle, IcChevronRight, IcDoc, IcNodes, IcScale, IcSearch, IcTarget } from "./Icons.tsx";

const ROLES = [
  { name: "Planner", desc: "Breaks down the task", Icon: IcDoc, phase: "deliberate" },
  { name: "Solvers", desc: "Searches and compares", Icon: IcNodes, phase: "act" },
  { name: "Critic", desc: "Checks gaps and risks", Icon: IcSearch, phase: "verify" },
  { name: "Judge", desc: "Validates and approves", Icon: IcScale, phase: "reflect" },
];

const EMPTY: Cognition = { phases: [], steps: [], stepsDone: 0 };

function statusFor(role: string, c: Cognition, streaming: boolean): { text: string; done: boolean } {
  const has = (p: string) => c.phases.some((x) => x.phase === p);
  switch (role) {
    case "Planner":
      if (c.steps.length > 1) return { text: `Plan: ${c.steps.length} stappen`, done: true };
      if (c.strategy)
        return { text: `${c.strategy.path === "fast" ? "Snel pad" : "Diep pad"} gekozen`, done: true };
      return { text: streaming ? "Overweegt…" : "Wacht", done: false };
    case "Solvers":
      if (c.steps.length > 1)
        return { text: `${c.stepsDone}/${c.steps.length} stappen`, done: c.stepsDone >= c.steps.length };
      if (has("act")) return { text: "Antwoord geschreven", done: true };
      return { text: streaming ? "Werkt…" : "Wacht", done: false };
    case "Critic":
      if (c.verify)
        return {
          text: c.verify.ok ? "Geen problemen" : `${c.verify.issues.length} punt(en)`,
          done: c.verify.ok,
        };
      return { text: "Wacht", done: false };
    default:
      if (has("reflect")) return { text: "Gereflecteerd", done: true };
      return { text: "Debatteam volgt in W09", done: false };
  }
}

export function AgentPanel() {
  const { c, streaming } = useStore((s) => {
    const list = s.currentSession ? s.messages[s.currentSession] : undefined;
    const last = list?.filter((m) => m.role === "assistant").at(-1);
    return {
      c: (last?.run_id ? s.cognition[last.run_id] : undefined) ?? EMPTY,
      streaming: last?.status === "streaming",
    };
  });
  const verdict = c.verify
    ? c.verify.ok
      ? "Gecontroleerd · geen problemen"
      : `Let op · ${c.verify.issues.join(", ")}`
    : streaming
      ? `Bezig · ${c.phases.at(-1)?.label ?? "start"}`
      : "Geen actieve beoordeling";
  return (
    <section className="panel agents">
      <h2 className="section-title">Agent collaboration</h2>
      <div className="agent-grid">
        {ROLES.map(({ name, desc, Icon }) => {
          const st = statusFor(name, c, streaming);
          return (
            <div key={name} style={{ display: "contents" }}>
              <div className={`agent-card ${streaming && !st.done ? "active" : ""}`}>
                <Icon size={30} />
                <div>
                  <div className="name">{name}</div>
                  <div className="desc">{desc}</div>
                </div>
              </div>
              <div className={`agent-status ${st.done ? "done" : ""}`}>
                {st.done ? (
                  <IcCheckCircle size={20} color="var(--vx-ok)" />
                ) : (
                  <i className={`dot ${streaming ? "accent" : "idle"}`} />
                )}
                {st.text}
              </div>
            </div>
          );
        })}
      </div>
      <div className="verdict" title={c.strategy?.why.join(" · ")}>
        <IcTarget size={26} color="var(--vx-accent)" />
        <span>Verdict</span>
        <span style={{ color: "var(--vx-muted)" }}>·</span>
        <span style={{ color: c.verify && !c.verify.ok ? "var(--vx-warn)" : "var(--vx-text-2)" }}>
          {verdict}
        </span>
        <IcChevronRight className="chev" />
      </div>
    </section>
  );
}
