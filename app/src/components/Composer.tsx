// Invoerbalk: bijlage, Quick/Deep/Research (Adaptive = geen keuze), modelkiezer, strategie, stop/verstuur.
// VX:W03.08
import { useRef, useState } from "react";
import type { Mode, ModelInfo } from "../api/types.ts";
import { setError } from "../state/store.ts";
import { IcBolt, IcChevron, IcLayers, IcModel, IcPlus, IcSearch, IcSliders } from "./Icons.tsx";

const MAX_ATTACH = 200_000;

interface Props {
  models: ModelInfo[];
  defaultModel: string;
  activeRun: string | null;
  onSend: (text: string, mode: Mode, model: string) => Promise<void>;
  onStop: () => Promise<void>;
  onSteer: (text: string) => Promise<void>;
}

export function Composer({ models, defaultModel, activeRun, onSend, onStop, onSteer }: Props) {
  const [text, setText] = useState("");
  const [mode, setMode] = useState<Mode>("adaptive");
  const [model, setModel] = useState("");
  const [attach, setAttach] = useState<{ name: string; body: string }[]>([]);
  const [busy, setBusy] = useState(false);
  const file = useRef<HTMLInputElement>(null);

  const toggle = (m: Mode) => setMode((cur) => (cur === m ? "adaptive" : m));

  const submit = async () => {
    const body = text.trim();
    if (!body || busy) return;
    if (activeRun) {
      // Tijdens een antwoord: bijsturen i.p.v. nieuw bericht (W07.10).
      setBusy(true);
      try {
        await onSteer(body);
        setText("");
      } catch (e) {
        setError(String(e));
      } finally {
        setBusy(false);
      }
      return;
    }
    const full =
      attach.length === 0
        ? body
        : `${body}\n\n${attach.map((a) => `Bijlage \`${a.name}\`:\n\n\`\`\`\n${a.body}\n\`\`\``).join("\n\n")}`;
    setBusy(true);
    try {
      await onSend(full, mode, model);
      setText("");
      setAttach([]);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const onFiles = async (files: FileList | null) => {
    for (const f of Array.from(files ?? [])) {
      if (f.size > MAX_ATTACH) {
        setError(`${f.name} is te groot (${Math.round(f.size / 1000)} kB). Grote documenten: ingest in W10.`);
        continue;
      }
      const body = await f.text();
      setAttach((a) => [...a, { name: f.name, body }]);
    }
  };

  return (
    <div className="composer">
      <textarea
        rows={1}
        placeholder={activeRun ? "Bijsturen tijdens het antwoord… (Enter)" : "Ask Vortex or assign a task..."}
        value={text}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && !e.shiftKey) {
            e.preventDefault();
            void submit();
          }
        }}
      />
      {attach.length > 0 && (
        <div className="row" style={{ gap: 8, marginTop: 6 }}>
          {attach.map((a, i) => (
            <button key={i} className="pill" onClick={() => setAttach((x) => x.filter((_, j) => j !== i))}>
              {a.name} ✕
            </button>
          ))}
        </div>
      )}
      <div className="composer-row">
        <button className="sq-btn" title="Bijlage (tekstbestanden)" onClick={() => file.current?.click()}>
          <IcPlus size={22} />
        </button>
        <input ref={file} type="file" multiple hidden onChange={(e) => void onFiles(e.target.files)} />
        <div className="modes">
          <button className={`mode ${mode === "quick" ? "active" : ""}`} onClick={() => toggle("quick")}>
            <IcBolt /> Quick
          </button>
          <button className={`mode ${mode === "deep" ? "active" : ""}`} onClick={() => toggle("deep")}>
            <IcLayers /> Deep
          </button>
          <button
            className={`mode ${mode === "research" ? "active" : ""}`}
            onClick={() => toggle("research")}
          >
            <IcSearch /> Research
          </button>
        </div>
        <div className="divider" />
        <label className="select">
          <IcModel />
          <select value={model} onChange={(e) => setModel(e.target.value)}>
            <option value="">{defaultModel || "standaardmodel"}</option>
            {models
              .filter((m) => m.name !== defaultModel)
              .map((m) => (
                <option key={m.name} value={m.name}>
                  {m.name}
                </option>
              ))}
          </select>
          <IcChevron size={18} />
        </label>
        <label className="select">
          <IcSliders />
          <select value={mode} onChange={(e) => setMode(e.target.value as Mode)}>
            <option value="adaptive">Adaptive</option>
            <option value="quick">Quick</option>
            <option value="deep">Deep</option>
            <option value="research">Research</option>
          </select>
          <IcChevron size={18} />
        </label>
        {activeRun ? (
          <button className="stop-btn" title="Stop (echt stoppen)" onClick={() => void onStop()}>
            <i />
          </button>
        ) : (
          <button
            className="stop-btn send"
            title="Verstuur (Enter)"
            disabled={!text.trim() || busy}
            onClick={() => void submit()}
          >
            <i />
          </button>
        )}
      </div>
    </div>
  );
}
