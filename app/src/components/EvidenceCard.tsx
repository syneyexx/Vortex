// Bewijsoverzicht per conclusie (W09.08): beweringen met label, bron, datum en scope.
import { useStore } from "../state/store.ts";

const LABEL: Record<string, [string, string]> = {
  supported: ["gestaafd", "ok"],
  contradicted: ["tegengesproken", "err"],
  unverified: ["onbevestigd", "run"],
};

export function EvidenceCard({ runId }: { runId: string }) {
  const ev = useStore((s) => s.cognition[runId]?.evidence);
  if (!ev || ev.checks.length === 0) return null;
  const ok = ev.checks.filter((c) => c.label === "supported").length;
  return (
    <details className="evidence">
      <summary>
        Bewijs: {ok}/{ev.checks.length} beweringen gestaafd ·{" "}
        {ev.scope === "lokaal" ? "lokaal geverifieerd" : "lokaal + web"}
      </summary>
      {ev.checks.map((c, i) => {
        const [txt, cls] = LABEL[c.label] ?? ["?", ""];
        const src = c.sources[0];
        return (
          <div key={i} className="evidence-row">
            <span className={`pill ${cls}`}>{txt}</span>
            <div>
              <div>{c.claim}</div>
              <div style={{ color: "var(--vx-muted)", fontSize: 12 }}>
                {c.why}
                {src && (
                  <>
                    {" "}
                    · {src.kind}:{" "}
                    {src.reference.startsWith("http") ? (
                      <a href={src.reference} target="_blank" rel="noreferrer">
                        {src.reference}
                      </a>
                    ) : (
                      src.reference
                    )}
                    {src.date ? ` (${src.date})` : ""}
                  </>
                )}
              </div>
            </div>
          </div>
        );
      })}
    </details>
  );
}
