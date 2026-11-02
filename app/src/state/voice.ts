// Spraak (W16.05): opnemen (MediaRecorder), base64, en antwoorden voorlezen per zin.
// Onderbreekbaar: `stopSpeaking()` of een nieuwe opname stopt het afspelen meteen.
import { API_BASE, api } from "../api/client.ts";

let queue: HTMLAudioElement[] = [];
let current: HTMLAudioElement | null = null;
let generation = 0;

export function toBase64(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const r = new FileReader();
    r.onload = () => resolve(String(r.result).split("base64,")[1] ?? "");
    r.onerror = () => reject(r.error ?? new Error("lezen mislukt"));
    r.readAsDataURL(blob);
  });
}

export function stopSpeaking() {
  generation++;
  current?.pause();
  current = null;
  queue = [];
}

export function isSpeaking() {
  return current !== null;
}

function playNext(gen: number) {
  if (gen !== generation) return;
  current = queue.shift() ?? null;
  if (!current) return;
  current.onended = () => playNext(gen);
  void current.play().catch(() => playNext(gen));
}

/// Tekst voorlezen (markdown/code eruit), zin voor zin.
export async function speak(text: string) {
  stopSpeaking();
  const gen = generation;
  const clean = text
    .replace(/```[\s\S]*?```/g, " (code) ")
    .replace(/!\[[^\]]*\]\([^)]*\)/g, " ")
    .replace(/[#*_`>|]/g, " ")
    .trim();
  if (!clean) return;
  const r = await api.speak(clean);
  if (gen !== generation) return;
  queue = r.parts.map((p) => new Audio(`${API_BASE}${p.url}`));
  playNext(gen);
}

/// Opname starten; de teruggegeven functie stopt en levert de transcriptie.
export async function record(): Promise<() => Promise<string>> {
  stopSpeaking(); // gebruiker praat → Vortex zwijgt
  const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
  const rec = new MediaRecorder(stream);
  const chunks: Blob[] = [];
  rec.ondataavailable = (e) => chunks.push(e.data);
  rec.start();
  return () =>
    new Promise((resolve, reject) => {
      rec.onstop = () => {
        stream.getTracks().forEach((t) => t.stop());
        const blob = new Blob(chunks, { type: rec.mimeType || "audio/webm" });
        toBase64(blob)
          .then((b64) => api.transcribe(rec.mimeType.includes("ogg") ? "spraak.ogg" : "spraak.webm", b64))
          .then((t) => resolve(t.text))
          .catch(reject);
      };
      rec.stop();
    });
}
