// Achtergrondtaak-balk: eerste lopende niet-chat-run (VX:W03.10). Pauzeren volgt met preemption (W04).
import { useState } from "react";
import { api } from "../api/client.ts";
import { useStore } from "../state/store.ts";
import { IcBook, IcPause, IcPlus, IcStop } from "./Icons.tsx";

export function TaskBar() {
  const run = useStore((s) => Object.values(s.runs).find((r) => r.kind !== "chat"));
  const [paused, setPaused] = useState(false);
  if (!run) return null;
  const toggle = async () => {
    if (paused) await api.resume(run.id);
    else await api.pause(run.id);
    setPaused(!paused);
  };
  const secs = Math.round((Date.now() - run.startedAt) / 1000);
  return (
    <div className="taskbar">
      <IcBook />
      <span>{run.kind}</span>
      <span className="muted">
        · {run.state} · {secs}s
      </span>
      {/* Voortgang onbekend → onbepaalde balk (geen verzonnen percentage). */}
      <div className="bar ind">
        <i />
      </div>
      <button
        className="icon-btn"
        title={paused ? "Hervatten" : "Pauzeren (op veilig punt)"}
        onClick={() => void toggle()}
      >
        {paused ? <IcPlus /> : <IcPause />}
      </button>
      <button className="icon-btn" title="Stoppen" onClick={() => void api.stop(run.id)}>
        <IcStop />
      </button>
    </div>
  );
}
