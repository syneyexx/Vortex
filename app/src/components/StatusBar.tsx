// Statusbalk: gezondheid per subsysteem, beide GPU's met VRAM, actief model, tok/s-sparkline (VX:W03.11).
import { useStore } from "../state/store.ts";
import { IcChip } from "./Icons.tsx";

function Sparkline({ values }: { values: number[] }) {
  const w = 190;
  const h = 34;
  if (values.length < 2) return <svg width={w} height={h} />;
  const max = Math.max(...values, 1);
  const pts = values
    .map((v, i) => `${(i / (values.length - 1)) * w},${h - 3 - (v / max) * (h - 8)}`)
    .join(" ");
  return (
    <svg width={w} height={h} className="spark">
      <defs>
        <linearGradient id="sg" x1="0" x2="0" y1="0" y2="1">
          <stop offset="0" stopColor="#ff5b1f" stopOpacity="0.35" />
          <stop offset="1" stopColor="#ff5b1f" stopOpacity="0" />
        </linearGradient>
      </defs>
      <polygon points={`0,${h} ${pts} ${w},${h}`} fill="url(#sg)" />
      <polyline points={pts} fill="none" stroke="#ff5b1f" strokeWidth="1.6" />
    </svg>
  );
}

function shortGpu(name: string): string {
  return name.replace(/^NVIDIA\s+/i, "").replace(/GeForce\s+/i, "");
}

export function StatusBar() {
  const st = useStore((s) => s.status);
  const conn = useStore((s) => s.conn);
  const tok = st?.tok_s ?? [];
  const main = st?.servers.find((s) => s.name === "main");
  return (
    <footer className="statusbar">
      <div className="engine">
        <IcChip size={24} />
        LOCAL ENGINE
        <span className="health" title={st?.subsystems.map((s) => `${s.name}: ${s.detail}`).join("\n")}>
          {(st?.subsystems ?? []).map((s) => (
            <i key={s.name} className={s.ok ? "ok" : ""} />
          ))}
        </span>
        <span className={`conn ${conn}`}>
          {conn === "open" ? "" : conn === "connecting" ? "verbinden…" : "verbinding weg"}
        </span>
      </div>
      {st && st.gpus.length > 0 ? (
        st.gpus.map((g) => (
          <div key={g.index}>
            <span>{shortGpu(g.name)}</span>
            <span className="vram">
              <i style={{ width: `${(100 * g.vram_used_mib) / Math.max(1, g.vram_total_mib)}%` }} />
            </span>
            <span className="muted">
              {(g.vram_used_mib / 1024).toFixed(1)} / {(g.vram_total_mib / 1024).toFixed(0)} GB
            </span>
          </div>
        ))
      ) : (
        <div className="muted">{st?.gpu_detail ? "Geen GPU-info (nvidia-smi)" : "GPU's laden…"}</div>
      )}
      <div>
        <IcChip size={24} />
        <span>{main?.model ?? st?.default_model ?? "geen model"}</span>
        <span className={main?.ready ? "" : "muted"}>{main?.ready ? "" : "· niet geladen"}</span>
      </div>
      <div style={{ flex: 1, borderLeft: "1px solid var(--vx-border-hi)" }}>
        <span>{tok.length ? `${(tok.at(-1) ?? 0).toFixed(0)} tok/s` : "— tok/s"}</span>
        <Sparkline values={tok} />
      </div>
    </footer>
  );
}
