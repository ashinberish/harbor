import { useEffect, useState } from "react";
import { api } from "../api";
import type { AppConfig, RestartPolicy } from "../api";

const RESTART_POLICIES: RestartPolicy[] = ["never", "on-failure", "always"];

interface EnvRow {
  key: string;
  value: string;
}

interface Props {
  name: string;
  onClose: () => void;
  onSaved: () => void;
}

export default function ConfigEditor({ name, onClose, onSaved }: Props) {
  const [config, setConfig] = useState<AppConfig | null>(null);
  const [command, setCommand] = useState("");
  const [envRows, setEnvRows] = useState<EnvRow[]>([]);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [validationError, setValidationError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    api
      .getAppConfig(name)
      .then((cfg) => {
        if (cancelled) return;
        setConfig(cfg);
        setCommand(cfg.command.join(" "));
        setEnvRows(Object.entries(cfg.env).map(([key, value]) => ({ key, value })));
      })
      .catch((e) => !cancelled && setError(String(e)))
      .finally(() => !cancelled && setLoading(false));
    return () => {
      cancelled = true;
    };
  }, [name]);

  function updateEnvRow(index: number, field: keyof EnvRow, value: string) {
    setEnvRows((rows) => rows.map((r, i) => (i === index ? { ...r, [field]: value } : r)));
  }

  function removeEnvRow(index: number) {
    setEnvRows((rows) => rows.filter((_, i) => i !== index));
  }

  function validate(cfg: AppConfig): string | null {
    if (!cfg.path.trim()) return "Path is required.";
    if (cfg.command.length === 0 || cfg.command.every((c) => !c.trim())) {
      return "Command cannot be empty.";
    }
    if (cfg.port !== null && (cfg.port < 1 || cfg.port > 65535)) {
      return "Port must be between 1 and 65535.";
    }
    if (cfg.restart_backoff_seconds > cfg.restart_max_backoff_seconds) {
      return "Restart backoff cannot exceed max backoff.";
    }
    return null;
  }

  async function handleSave() {
    if (!config) return;
    const env: Record<string, string> = {};
    for (const row of envRows) {
      if (row.key.trim()) env[row.key.trim()] = row.value;
    }
    const updated: AppConfig = {
      ...config,
      command: command.trim().split(/\s+/).filter(Boolean),
      env,
    };
    const problem = validate(updated);
    if (problem) {
      setValidationError(problem);
      return;
    }
    setValidationError(null);
    setSaving(true);
    setError(null);
    try {
      await api.updateAppConfig(updated);
      onSaved();
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          <h2>Configure — {name}</h2>
          <button className="icon-button" onClick={onClose} aria-label="Close">
            ×
          </button>
        </div>

        {loading && <p className="muted">Loading…</p>}
        {error && <div className="error-banner">{error}</div>}
        {validationError && <div className="error-banner">{validationError}</div>}

        {config && !loading && (
          <div className="modal-body">
            <label>
              Path
              <input
                value={config.path}
                onChange={(e) => setConfig({ ...config, path: e.target.value })}
              />
            </label>
            <label>
              Command <span className="muted">(space-separated)</span>
              <input value={command} onChange={(e) => setCommand(e.target.value)} />
            </label>
            <label>
              Working directory <span className="muted">(optional, defaults to path)</span>
              <input
                value={config.working_dir ?? ""}
                onChange={(e) => setConfig({ ...config, working_dir: e.target.value || null })}
              />
            </label>
            <div className="field-row">
              <label>
                Port
                <input
                  value={config.port ?? ""}
                  onChange={(e) =>
                    setConfig({
                      ...config,
                      port: e.target.value ? Number(e.target.value.replace(/[^0-9]/g, "")) : null,
                    })
                  }
                />
              </label>
              <label>
                Domain
                <input
                  value={config.domain ?? ""}
                  onChange={(e) => setConfig({ ...config, domain: e.target.value || null })}
                />
              </label>
              <label>
                Path prefix
                <input
                  value={config.path_prefix ?? ""}
                  onChange={(e) => setConfig({ ...config, path_prefix: e.target.value || null })}
                />
              </label>
            </div>
            <label>
              Restart policy
              <select
                value={config.restart_policy}
                onChange={(e) => setConfig({ ...config, restart_policy: e.target.value as RestartPolicy })}
              >
                {RESTART_POLICIES.map((p) => (
                  <option key={p} value={p}>
                    {p}
                  </option>
                ))}
              </select>
            </label>
            <div className="field-row">
              <label>
                Restart backoff (s)
                <input
                  value={config.restart_backoff_seconds}
                  onChange={(e) =>
                    setConfig({
                      ...config,
                      restart_backoff_seconds: Number(e.target.value.replace(/[^0-9]/g, "") || "0"),
                    })
                  }
                />
              </label>
              <label>
                Max backoff (s)
                <input
                  value={config.restart_max_backoff_seconds}
                  onChange={(e) =>
                    setConfig({
                      ...config,
                      restart_max_backoff_seconds: Number(e.target.value.replace(/[^0-9]/g, "") || "0"),
                    })
                  }
                />
              </label>
            </div>

            <div className="env-editor">
              <div className="env-editor-header">
                <span>Environment variables</span>
                <button type="button" onClick={() => setEnvRows((rows) => [...rows, { key: "", value: "" }])}>
                  + Add
                </button>
              </div>
              {envRows.map((row, i) => (
                <div key={i} className="env-row">
                  <input value={row.key} onChange={(e) => updateEnvRow(i, "key", e.target.value)} placeholder="KEY" />
                  <input
                    value={row.value}
                    onChange={(e) => updateEnvRow(i, "value", e.target.value)}
                    placeholder="value"
                  />
                  <button type="button" className="icon-button" onClick={() => removeEnvRow(i)}>
                    ×
                  </button>
                </div>
              ))}
            </div>

            <p className="hint muted">
              Changes to command, env, or working directory take effect on the app's next start or restart.
            </p>

            <div className="modal-footer">
              <button type="button" onClick={onClose}>
                Cancel
              </button>
              <button type="button" className="primary" disabled={saving} onClick={handleSave}>
                {saving ? "Saving…" : "Save"}
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
