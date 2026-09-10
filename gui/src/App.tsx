import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "./api";
import type { AppStatus } from "./api";
import AppTable from "./components/AppTable";
import AddAppWizard from "./components/AddAppWizard";
import LogViewer from "./components/LogViewer";
import ConfigEditor from "./components/ConfigEditor";
import "./App.css";

const POLL_MS = 2000;

function App() {
  const [apps, setApps] = useState<AppStatus[]>([]);
  const [connectionError, setConnectionError] = useState<string | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);

  const [showWizard, setShowWizard] = useState(false);
  const [logsFor, setLogsFor] = useState<string | null>(null);
  const [configFor, setConfigFor] = useState<string | null>(null);

  const pollTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);

  const refresh = useCallback(async () => {
    try {
      const list = await api.listApps();
      setApps(list);
      setConnectionError(null);
    } catch (e) {
      setConnectionError(String(e));
    } finally {
      setLoaded(true);
    }
  }, []);

  useEffect(() => {
    let cancelled = false;
    async function loop() {
      await refresh();
      if (!cancelled) pollTimer.current = setTimeout(loop, POLL_MS);
    }
    loop();
    return () => {
      cancelled = true;
      clearTimeout(pollTimer.current);
    };
  }, [refresh]);

  async function withBusy(name: string, action: () => Promise<void>) {
    setBusy(name);
    setActionError(null);
    try {
      await action();
      await refresh();
    } catch (e) {
      setActionError(String(e));
    } finally {
      setBusy(null);
    }
  }

  return (
    <main className="app-shell">
      <header className="app-header">
        <div className="brand">
          <span className="brand-mark">⛴</span>
          <span className="brand-name">Harbor</span>
        </div>
        <button className="primary" onClick={() => setShowWizard(true)}>
          + Add App
        </button>
      </header>

      {connectionError && (
        <div className="error-banner">
          Can't reach the Harbor daemon: {connectionError}
          <div className="muted">Is <code>harbord</code> running?</div>
        </div>
      )}
      {actionError && <div className="error-banner">{actionError}</div>}

      <section className="app-content">
        {!loaded ? (
          <p className="muted">Loading…</p>
        ) : (
          <AppTable
            apps={apps}
            busy={busy}
            onStart={(name) => withBusy(name, () => api.startApp(name).then(() => undefined))}
            onStop={(name) => withBusy(name, () => api.stopApp(name).then(() => undefined))}
            onRestart={(name) => withBusy(name, () => api.restartApp(name).then(() => undefined))}
            onRemove={(name) => withBusy(name, () => api.removeApp(name).then(() => undefined))}
            onLogs={(name) => setLogsFor(name)}
            onConfig={(name) => setConfigFor(name)}
          />
        )}
      </section>

      {showWizard && (
        <AddAppWizard
          onClose={() => setShowWizard(false)}
          onAdded={() => {
            setShowWizard(false);
            refresh();
          }}
        />
      )}
      {logsFor && <LogViewer name={logsFor} onClose={() => setLogsFor(null)} />}
      {configFor && (
        <ConfigEditor
          name={configFor}
          onClose={() => setConfigFor(null)}
          onSaved={() => {
            setConfigFor(null);
            refresh();
          }}
        />
      )}
    </main>
  );
}

export default App;
