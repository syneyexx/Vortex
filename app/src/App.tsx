// VX:W03.05 — app-shell en routing.
import { useEffect, useState } from "react";
import { ChatPane } from "./components/ChatPane.tsx";
import { LeftColumn } from "./components/LeftColumn.tsx";
import { DashboardPage } from "./pages/DashboardPage.tsx";
import { MemoryPage } from "./pages/MemoryPage.tsx";
import { ResearchPage } from "./pages/ResearchPage.tsx";
import { TrainingPage } from "./pages/TrainingPage.tsx";
import { StatusBar } from "./components/StatusBar.tsx";
import { SubNav, TopNav, type Sub, type Top } from "./components/TopNav.tsx";
import { ModelsPage, Placeholder, RunsPage, SettingsPage } from "./pages/Pages.tsx";
import { connect, dismissToast, openSession, setError, useStore } from "./state/store.ts";

function Toasts() {
  const toasts = useStore((s) => s.toasts);
  useEffect(() => {
    // Systeemnotificatie voor achtergrondtaken (als toegestaan), anders alleen in-app. VX:W03.14
    const t = toasts.at(-1);
    if (t && document.hidden && "Notification" in window && Notification.permission === "granted") {
      new Notification("Vortex", { body: t.text });
    }
    const timers = toasts.map((x) => setTimeout(() => dismissToast(x.id), 6000));
    return () => timers.forEach(clearTimeout);
  }, [toasts]);
  return (
    <div className="toasts">
      {toasts.map((t) => (
        <div key={t.id} className="toast" onClick={() => dismissToast(t.id)}>
          {t.text}
        </div>
      ))}
    </div>
  );
}

export function App() {
  const [top, setTop] = useState<Top>("chat");
  const [sub, setSub] = useState<Sub>("chat");
  const error = useStore((s) => s.error);
  const sessions = useStore((s) => s.sessions);
  const current = useStore((s) => s.currentSession);

  useEffect(() => {
    void connect();
    if ("Notification" in window && Notification.permission === "default")
      void Notification.requestPermission();
  }, []);
  // Laatste gesprek automatisch openen.
  useEffect(() => {
    if (!current && sessions[0]) void openSession(sessions[0].id);
  }, [current, sessions]);

  let body;
  if (top === "chat" && sub === "chat") {
    body = (
      <>
        <LeftColumn />
        <ChatPane />
      </>
    );
  } else if (top === "chat" && sub === "dashboard") body = <DashboardPage />;
  else if (top === "chat")
    body = (
      <Placeholder
        title="Agents"
        wave="W09 ✓"
        text="Het debatteam draait live in het Agent collaboration-paneel; rollen en prompts bewerk je in Instellingen (config/roles/)."
      />
    );
  else if (top === "research") body = <ResearchPage />;
  else if (top === "memory") body = <MemoryPage />;
  else if (top === "models")
    body = (
      <div style={{ display: "grid", gap: 0 }}>
        <ModelsPage />
        <TrainingPage />
      </div>
    );
  else if (top === "studio")
    body = <Placeholder title="Studio" wave="W16" text="Beeld, audio en multimodale uitvoer." />;
  else if (top === "settings") body = <SettingsPage />;
  else body = <RunsPage />;

  return (
    <div className="app">
      <TopNav
        top={top}
        onTop={(t) => {
          setTop(t);
          if (t === "chat") setSub("chat");
        }}
      />
      <SubNav
        sub={sub}
        onSub={(s) => {
          setSub(s);
          setTop("chat");
        }}
      />
      <main className="main">{body}</main>
      <StatusBar />
      {error && (
        <div className="error-banner" onClick={() => setError(null)}>
          {error}
        </div>
      )}
      <Toasts />
    </div>
  );
}
