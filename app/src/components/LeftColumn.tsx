// Linkerkolom: vortex-hero, Agent collaboration + Verdict, Retrieved memory.
// Toont alleen echte toestand: agents en geheugen zijn leeg tot hun wave (W09 / W05).
import { useStore } from "../state/store.ts";
import {
  IcBook,
  IcChevronRight,
  IcClock,
  IcDb,
  IcDoc,
  IcNodes,
  IcScale,
  IcSearch,
  IcTarget,
} from "./Icons.tsx";
import { VortexShader } from "./VortexShader.tsx";

const ROLES = [
  { name: "Planner", desc: "Breaks down the task", Icon: IcDoc },
  { name: "Solvers", desc: "Searches and compares", Icon: IcNodes },
  { name: "Critic", desc: "Checks gaps and risks", Icon: IcSearch },
  { name: "Judge", desc: "Validates and approves", Icon: IcScale },
];

export function LeftColumn() {
  const streaming = useStore((s) =>
    Object.values(s.messages).some((l) => l.some((m) => m.status === "streaming")),
  );
  const conn = useStore((s) => s.conn);
  return (
    <aside className="left">
      <div className="hero">
        <VortexShader center={[0.4, 0.42]} zoom={0.9} />
        <div className="hero-title">
          Intelligence
          <br />
          in motion
        </div>
        <div className="hero-legend">
          <span>
            <i className={`dot ${streaming ? "accent" : "idle"}`} />
            Agents active
          </span>
          <span>
            <i className={`dot ${conn === "open" ? "white" : "idle"}`} />
            Knowledge sync
          </span>
        </div>
      </div>

      <section className="panel agents">
        <h2 className="section-title">Agent collaboration</h2>
        <div className="agent-grid">
          {ROLES.map(({ name, desc, Icon }) => (
            <div key={name} style={{ display: "contents" }}>
              <div className="agent-card">
                <Icon size={30} />
                <div>
                  <div className="name">{name}</div>
                  <div className="desc">{desc}</div>
                </div>
              </div>
              <div className="agent-status">
                <i className="dot idle" />
                Inactief · volgt in W09
              </div>
            </div>
          ))}
        </div>
        <div className="verdict">
          <IcTarget size={26} color="var(--vx-accent)" />
          <span>Verdict</span>
          <span style={{ color: "var(--vx-muted)" }}>·</span>
          <span style={{ color: "var(--vx-text-2)" }}>
            {streaming ? "Single agent antwoordt" : "Geen actieve beoordeling"}
          </span>
          <IcChevronRight className="chev" />
        </div>
      </section>

      <section className="panel memory">
        <div>
          <h2 className="section-title">Retrieved memory</h2>
          {[
            ["Semantic", IcBook],
            ["Episodic", IcClock],
            ["Procedural", IcDb],
          ].map(([label, Icon]) => {
            const I = Icon as typeof IcBook;
            return (
              <div className="mem-row" key={label as string}>
                <I size={20} />
                <span>
                  {label as string} <span style={{ color: "var(--vx-muted)" }}>· nog niets opgehaald</span>
                </span>
                <span className="chip">—</span>
              </div>
            );
          })}
        </div>
        <div style={{ display: "grid", alignContent: "center", justifyItems: "center", gap: 8 }}>
          <svg width="110" height="100" viewBox="0 0 110 100" aria-label="Knowledge graph (leeg)">
            <circle cx="55" cy="50" r="7" fill="none" stroke="#3a3a42" />
          </svg>
          <div className="graph-cap">Knowledge graph · W05</div>
        </div>
      </section>
    </aside>
  );
}
