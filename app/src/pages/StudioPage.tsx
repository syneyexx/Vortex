// Studio (W16): beeld maken/bewerken (LLM wordt eerst van de GPU gehaald), tekst voorlezen,
// en een bestand (beeld, audio, video, document) analyseren → gaat ook het geheugen in.
import { useEffect, useState } from "react";
import { API_BASE, api } from "../api/client.ts";
import { setError } from "../state/store.ts";
import { speak, stopSpeaking, toBase64 } from "../state/voice.ts";

export function StudioPage() {
  const [st, setSt] = useState<Awaited<ReturnType<typeof api.media>> | null>(null);
  const [prompt, setPrompt] = useState("");
  const [edit, setEdit] = useState<{ name: string; path: string } | null>(null);
  const [images, setImages] = useState<{ url: string; prompt: string }[]>([]);
  const [say, setSay] = useState("");
  const [analysis, setAnalysis] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  useEffect(() => {
    api
      .media()
      .then(setSt)
      .catch(() => undefined);
  }, []);
  const up = async (f: File) => api.upload(f.name, await toBase64(f));
  return (
    <div className="page">
      <h1>Studio</h1>
      <p style={{ color: "var(--vx-muted)", fontSize: 13 }}>
        {st
          ? `Visie: ${st.vision ? "ja" : "geen model (rol vision)"} · OCR ${st.ocr_engine} · spraak ${st.stt_engine} / ${st.tts_engine} · beeld ${st.imagegen_engine}`
          : "–"}{" "}
        · In de chat: bijlagen via +, 🎙 voor spraak, <code>/beeld …</code> voor een beeld.
      </p>
      <div className="row" style={{ alignItems: "flex-start", gap: 20, flexWrap: "wrap" }}>
        <div className="panel card" style={{ flex: 2, minWidth: 360 }}>
          <h3>Beeld maken of bewerken</h3>
          <textarea
            value={prompt}
            onChange={(e) => setPrompt(e.target.value)}
            placeholder="Beschrijf het beeld…"
            style={{ width: "100%", minHeight: 70 }}
          />
          <div className="row" style={{ gap: 8, marginTop: 6 }}>
            <label className="btn">
              {edit ? `bewerk: ${edit.name}` : "beeld bewerken…"}
              <input
                type="file"
                accept="image/*"
                hidden
                onChange={(e) => {
                  const f = e.target.files?.[0];
                  if (f)
                    void up(f)
                      .then((u) => setEdit({ name: f.name, path: u.path }))
                      .catch((x: unknown) => setError(String(x)));
                }}
              />
            </label>
            {edit && (
              <button className="btn" onClick={() => setEdit(null)}>
                ✕
              </button>
            )}
            <button
              className="btn primary"
              disabled={!prompt || !!busy}
              onClick={() => {
                setBusy("img");
                api
                  .imagine(prompt, edit?.path)
                  .then((r) => setImages((x) => [{ url: `${API_BASE}${r.url}`, prompt }, ...x]))
                  .catch((e: unknown) => setError(String(e)))
                  .finally(() => setBusy(null));
              }}
            >
              {busy === "img" ? "bezig (LLM ontladen)…" : "Maken"}
            </button>
          </div>
          <div className="row" style={{ gap: 10, flexWrap: "wrap", marginTop: 12 }}>
            {images.map((i) => (
              <figure key={i.url} style={{ margin: 0, maxWidth: 240 }}>
                <img src={i.url} alt={i.prompt} style={{ width: "100%", borderRadius: 8 }} />
                <figcaption style={{ fontSize: 12, color: "var(--vx-muted)" }}>{i.prompt}</figcaption>
              </figure>
            ))}
          </div>
        </div>
        <div className="panel card" style={{ flex: 1, minWidth: 300 }}>
          <h3>Voorlezen</h3>
          <textarea
            value={say}
            onChange={(e) => setSay(e.target.value)}
            style={{ width: "100%", minHeight: 70 }}
          />
          <div className="row" style={{ gap: 6, marginTop: 6 }}>
            <button
              className="btn"
              disabled={!say}
              onClick={() => void speak(say).catch((e: unknown) => setError(String(e)))}
            >
              🔊 Voorlezen
            </button>
            <button className="btn" onClick={() => stopSpeaking()}>
              Stop
            </button>
          </div>
          <h3 style={{ marginTop: 18 }}>Bestand analyseren</h3>
          <input
            type="file"
            onChange={(e) => {
              const f = e.target.files?.[0];
              if (!f) return;
              setBusy("an");
              setAnalysis(null);
              void up(f)
                .then((u) => api.analyze(u.path))
                .then((r) => setAnalysis(r.text))
                .catch((x: unknown) => setError(String(x)))
                .finally(() => setBusy(null));
            }}
          />
          {busy === "an" && <p style={{ fontSize: 12 }}>bezig…</p>}
          {analysis && (
            <pre style={{ whiteSpace: "pre-wrap", fontSize: 12, maxHeight: 360, overflow: "auto" }}>
              {analysis}
            </pre>
          )}
        </div>
      </div>
    </div>
  );
}
