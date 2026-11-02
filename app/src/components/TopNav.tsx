// Topnav + subtabs (VX:W03.05).
import { IcGear, IcMonitor } from "./Icons.tsx";
import { VortexMark } from "./VortexShader.tsx";

export type Top =
  "chat" | "research" | "memory" | "models" | "studio" | "autonomy" | "workspace" | "settings" | "runs";
export type Sub = "dashboard" | "chat" | "agents";

const TOPS: [Top, string][] = [
  ["chat", "Chat"],
  ["research", "Research"],
  ["memory", "Memory"],
  ["models", "Models"],
  ["studio", "Studio"],
  ["autonomy", "Autonomie"],
];

export function TopNav({ top, onTop }: { top: Top; onTop: (t: Top) => void }) {
  return (
    <nav className="topnav">
      <div className="logo">
        <div className="logo-mark">
          <VortexMark size={44} />
        </div>
        <span className="logo-text">VORTEX</span>
      </div>
      <div className="tabs">
        {TOPS.map(([k, label]) => (
          <button key={k} className={`tab ${top === k ? "active" : ""}`} onClick={() => onTop(k)}>
            {label}
          </button>
        ))}
      </div>
      <div className="topnav-right">
        <button onClick={() => onTop("runs")} title="Lopend werk">
          <IcMonitor size={24} /> Local workspace
        </button>
        <button onClick={() => onTop("settings")}>
          <IcGear size={24} /> System prompt
        </button>
      </div>
    </nav>
  );
}

export function SubNav({ sub, onSub }: { sub: Sub; onSub: (s: Sub) => void }) {
  return (
    <nav className="subnav">
      {(
        [
          ["dashboard", "Dashboard"],
          ["chat", "Chat"],
          ["agents", "Agents"],
        ] as [Sub, string][]
      ).map(([k, label]) => (
        <button key={k} className={`tab ${sub === k ? "active" : ""}`} onClick={() => onSub(k)}>
          {label}
        </button>
      ))}
    </nav>
  );
}
