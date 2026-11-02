// Eventstream met hervatten: onthoudt het laatste volgnummer; na een verbroken verbinding
// verbindt hij opnieuw met from_seq (exponentiële backoff). Frames worden per animatie-frame
// gebundeld aan de store gegeven (UI-event coalescing, 60 fps).
import { API_BASE } from "./client.ts";
import type { VxEvent } from "./types.ts";

export type ConnState = "connecting" | "open" | "closed";

export class EventStream {
  private ws: WebSocket | null = null;
  private lastSeq: number;
  private backoff = 250;
  private pending: VxEvent[] = [];
  private raf = 0;
  private stopped = false;

  constructor(
    startSeq: number,
    private onBatch: (events: VxEvent[]) => void,
    private onState: (s: ConnState) => void,
  ) {
    this.lastSeq = startSeq;
  }

  get seq(): number {
    return this.lastSeq;
  }

  start(): void {
    this.stopped = false;
    this.connect();
  }

  stop(): void {
    this.stopped = true;
    this.ws?.close();
    cancelAnimationFrame(this.raf);
  }

  private connect(): void {
    if (this.stopped) return;
    this.onState("connecting");
    const url = `${API_BASE.replace(/^http/, "ws")}/api/events?from_seq=${this.lastSeq}`;
    const ws = new WebSocket(url);
    this.ws = ws;
    ws.onopen = () => {
      this.backoff = 250;
      this.onState("open");
    };
    ws.onmessage = (m) => {
      const e = JSON.parse(String(m.data)) as VxEvent;
      if (e.seq <= this.lastSeq) return; // dubbel na reconnect
      this.lastSeq = e.seq;
      this.pending.push(e);
      if (!this.raf) this.raf = requestAnimationFrame(() => this.flush());
    };
    ws.onclose = () => {
      this.onState("closed");
      if (this.stopped) return;
      setTimeout(() => this.connect(), this.backoff);
      this.backoff = Math.min(this.backoff * 2, 5000);
    };
  }

  private flush(): void {
    this.raf = 0;
    const batch = this.pending;
    this.pending = [];
    if (batch.length) this.onBatch(batch);
  }
}
