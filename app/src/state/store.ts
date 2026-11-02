// Globale store (useSyncExternalStore) + verbinding met de kern.
import { useSyncExternalStore } from "react";
import { api } from "../api/client.ts";
import { EventStream, type ConnState } from "../api/events.ts";
import type { Status, VxEvent } from "../api/types.ts";
import { applyEvents, initialState, type State } from "./reduce.ts";

export interface Store extends State {
  conn: ConnState;
  status: Status | null;
  error: string | null;
}

let state: Store = { ...initialState, conn: "connecting", status: null, error: null };
const listeners = new Set<() => void>();

function set(next: Partial<Store>): void {
  state = { ...state, ...next };
  listeners.forEach((l) => l());
}

export function getState(): Store {
  return state;
}

export function useStore<T>(sel: (s: Store) => T): T {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    () => sel(state),
  );
}

async function refreshSession(id: string): Promise<void> {
  try {
    const r = await api.session(id);
    // Streaming-berichten niet overschrijven met een oudere server-versie.
    const local = state.messages[id] ?? [];
    const merged = r.messages.map((m) => {
      const l = local.find((x) => x.id === m.id);
      return l && l.status === "streaming" && m.status === "streaming" && l.content.length > m.content.length
        ? l
        : m;
    });
    set({ messages: { ...state.messages, [id]: merged } });
  } catch (e) {
    set({ error: String(e) });
  }
}

export async function refreshSessions(): Promise<void> {
  try {
    set({ sessions: await api.sessions() });
  } catch (e) {
    set({ error: String(e) });
  }
}

function onBatch(events: VxEvent[]): void {
  const next = applyEvents(state, events);
  const dirty = next.dirty;
  set({ ...next, dirty: [] });
  for (const d of new Set(dirty)) {
    if (d === "__sessions") void refreshSessions();
    else if (state.messages[d]) void refreshSession(d);
  }
  if (dirty.length) void refreshSessions();
}

let stream: EventStream | null = null;

export async function connect(): Promise<void> {
  if (stream) return;
  // Snapshot eerst, daarna events vanaf het snapshot-volgnummer.
  let seq = 0;
  try {
    const st = await api.status();
    seq = st.last_seq;
    set({ status: st });
  } catch (e) {
    set({ error: `Kern niet bereikbaar: ${String(e)}` });
  }
  await refreshSessions();
  stream = new EventStream(seq, onBatch, (c) => set({ conn: c }));
  stream.start();
  setInterval(async () => {
    try {
      set({ status: await api.status(), error: null });
    } catch {
      /* verbinding wordt door de stream gemeld */
    }
  }, 2000);
}

export async function openSession(id: string | null): Promise<void> {
  set({ currentSession: id });
  if (id) await refreshSession(id);
}

export async function newSession(): Promise<string> {
  const s = await api.createSession();
  set({ messages: { ...state.messages, [s.id]: [] } });
  await refreshSessions();
  await openSession(s.id);
  return s.id;
}

export function dismissToast(id: number): void {
  set({ toasts: state.toasts.filter((t) => t.id !== id) });
}

export function setError(e: string | null): void {
  set({ error: e });
}
