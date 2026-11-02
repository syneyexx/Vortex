// Markdown met codeblokken (highlighting, kopiëren, diff), tabellen, LaTeX en Mermaid (VX:W03.07).
import { memo, useEffect, useId, useRef, useState, type ReactNode } from "react";
import ReactMarkdown from "react-markdown";
import rehypeHighlight from "rehype-highlight";
import rehypeKatex from "rehype-katex";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import "highlight.js/styles/github-dark.css";
import "katex/dist/katex.min.css";

function textOf(node: ReactNode): string {
  if (typeof node === "string" || typeof node === "number") return String(node);
  if (Array.isArray(node)) return node.map(textOf).join("");
  if (node && typeof node === "object" && "props" in node) {
    return textOf((node as { props: { children?: ReactNode } }).props.children);
  }
  return "";
}

function Pre({ children }: { children?: ReactNode }) {
  const [copied, setCopied] = useState(false);
  const copy = async () => {
    await navigator.clipboard.writeText(textOf(children));
    setCopied(true);
    setTimeout(() => setCopied(false), 1200);
  };
  return (
    <pre>
      <button className="code-copy" onClick={() => void copy()}>
        {copied ? "gekopieerd" : "kopieer"}
      </button>
      {children}
    </pre>
  );
}

function Mermaid({ code }: { code: string }) {
  const ref = useRef<HTMLDivElement>(null);
  const id = useId().replace(/:/g, "");
  const [err, setErr] = useState<string | null>(null);
  useEffect(() => {
    let alive = true;
    void (async () => {
      try {
        const { default: mermaid } = await import("mermaid");
        mermaid.initialize({ startOnLoad: false, theme: "dark", securityLevel: "strict" });
        const { svg } = await mermaid.render(`m${id}`, code);
        if (alive && ref.current) ref.current.innerHTML = svg;
      } catch (e) {
        if (alive) setErr(String(e));
      }
    })();
    return () => {
      alive = false;
    };
  }, [code, id]);
  return err ? <pre>{code}</pre> : <div ref={ref} className="mermaid" />;
}

function Code({ className, children }: { className?: string; children?: ReactNode }) {
  if (className?.includes("language-mermaid")) return <Mermaid code={textOf(children)} />;
  return <code className={className}>{children}</code>;
}

export const Markdown = memo(function Markdown({ text, streaming }: { text: string; streaming: boolean }) {
  return (
    <div className="prose">
      <ReactMarkdown
        remarkPlugins={[remarkGfm, remarkMath]}
        // Tijdens streamen geen highlighting/LaTeX per frame: pas bij afronden (60 fps).
        rehypePlugins={
          streaming ? [] : [[rehypeHighlight, { detect: true, ignoreMissing: true }], rehypeKatex]
        }
        components={{ pre: Pre, code: Code }}
      >
        {text}
      </ReactMarkdown>
    </div>
  );
});
