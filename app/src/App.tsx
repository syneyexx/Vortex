// VX:W00.12 — skelet. De echte GUI (1-op-1 volgens VAULT/design/reference-chat.png) volgt in W03.
import { RunState } from "./gen/vortex/common/v1/common_pb.ts";

export function App() {
  return (
    <main
      style={{
        fontFamily: "system-ui, sans-serif",
        padding: 24,
        background: "#0b0b0c",
        color: "#e8e8e8",
        minHeight: "100vh",
      }}
    >
      <h1 style={{ letterSpacing: "0.3em" }}>VORTEX</h1>
      <p>Skelet (W00). Contracten geladen: {Object.keys(RunState).length / 2} run-statussen.</p>
    </main>
  );
}
