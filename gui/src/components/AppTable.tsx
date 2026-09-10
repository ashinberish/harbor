import type { AppStatus } from "../api";
import { formatBytes, formatUptime } from "../api";
import StatusBadge from "./StatusBadge";

interface Props {
  apps: AppStatus[];
  busy: string | null;
  onStart: (name: string) => void;
  onStop: (name: string) => void;
  onRestart: (name: string) => void;
  onRemove: (name: string) => void;
  onLogs: (name: string) => void;
  onConfig: (name: string) => void;
}

export default function AppTable({
  apps,
  busy,
  onStart,
  onStop,
  onRestart,
  onRemove,
  onLogs,
  onConfig,
}: Props) {
  if (apps.length === 0) {
    return (
      <div className="empty-state">
        <p>No apps yet.</p>
        <p className="muted">Click "Add App" to start managing your first app.</p>
      </div>
    );
  }

  return (
    <table className="app-table">
      <thead>
        <tr>
          <th>Name</th>
          <th>Status</th>
          <th>Runtime</th>
          <th>PID</th>
          <th>CPU%</th>
          <th>Memory</th>
          <th>Uptime</th>
          <th>Restarts</th>
          <th>Address</th>
          <th></th>
        </tr>
      </thead>
      <tbody>
        {apps.map((app) => {
          const isBusy = busy === app.name;
          const running = app.state === "running" || app.state === "restarting";
          const address = app.domain
            ? `${app.domain}${app.path_prefix ?? ""}`
            : app.port
              ? `:${app.port}`
              : "—";
          return (
            <tr key={app.name}>
              <td className="app-name">{app.name}</td>
              <td>
                <StatusBadge state={app.state} />
              </td>
              <td>{app.runtime}</td>
              <td>{app.pid ?? "—"}</td>
              <td>{app.cpu_percent !== null ? `${app.cpu_percent.toFixed(1)}%` : "—"}</td>
              <td>{formatBytes(app.memory_bytes)}</td>
              <td>{formatUptime(app.uptime_seconds)}</td>
              <td>{app.restart_count}</td>
              <td className="muted">{address}</td>
              <td className="actions">
                {running ? (
                  <button disabled={isBusy} onClick={() => onStop(app.name)}>
                    Stop
                  </button>
                ) : (
                  <button disabled={isBusy} onClick={() => onStart(app.name)}>
                    Start
                  </button>
                )}
                <button disabled={isBusy} onClick={() => onRestart(app.name)}>
                  Restart
                </button>
                <button disabled={isBusy} onClick={() => onLogs(app.name)}>
                  Logs
                </button>
                <button disabled={isBusy} onClick={() => onConfig(app.name)}>
                  Config
                </button>
                <button
                  disabled={isBusy}
                  className="danger"
                  onClick={() => {
                    if (window.confirm(`Remove "${app.name}"? This stops it and deletes its config.`)) {
                      onRemove(app.name);
                    }
                  }}
                >
                  Remove
                </button>
              </td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}
