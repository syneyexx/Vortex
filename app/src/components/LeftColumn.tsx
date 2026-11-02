// Linkerkolom: vortex-hero, Agent collaboration + Verdict, Retrieved memory.
// Toont alleen echte toestand: agents en geheugen zijn leeg tot hun wave (W09 / W05).
import { useStore } from "../state/store.ts";
import { AgentPanel } from "./AgentPanel.tsx";
import { MemoryPanel } from "./MemoryPanel.tsx";
import { VortexShader } from "./VortexShader.tsx";

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

      <AgentPanel />

      <MemoryPanel />
    </aside>
  );
}
