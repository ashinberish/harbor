import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api } from "../api";
import type { RestartPolicy, RuntimeKind } from "../api";

const RUNTIMES: RuntimeKind[] = ["python", "node", "dotnet", "java", "rust", "custom"];
const RESTART_POLICIES: RestartPolicy[] = ["never", "on-failure", "always"];

interface EnvRow {
  key: string;
  value: string;
}

interface Props {
  onClose: () => void;
  onAdded: () => void;
}

export default function AddAppWizard({ onClose, onAdded }: Props) {
  const [step, setStep] = useState<1 | 2>(1);
  const [path, setPath] = useState("");
  const [detectedRuntime, setDetectedRuntime] = useState<RuntimeKind | null>(null);
  const [detecting, setDetecting] = useState(false);

  const [name, setName] = useState("");
  const [runtime, setRuntime] = useState<RuntimeKind | "">("");
  const [command, setCommand] = useState("");
  const [port, setPort] = useState("");
  const [domain, setDomain] = useState("");
  const [pathPrefix, setPathPrefix] = useState("");
  const [restartPolicy, setRestartPolicy] = useState<RestartPolicy>("on-failure");
  const [envRows, setEnvRows] = useState<EnvRow[]>([]);

  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function handleBrowse() {
    const selected = await open({ directory: true, multiple: false });
    if (typeof selected === "string") {
      setPath(selected);
      setDetectedRuntime(null);
    }
  }

  async function handleDetect() {
    if (!path.trim()) return;
    setDetecting(true);
    setError(null);
    try {
      const detected = await api.detectRuntime(path.trim());
      setDetectedRuntime(detected);
      if (detected) setRuntime(detected);
    } catch (e) {
      setError(String(e));
    } finally {
      setDetecting(false);
    }
  }

  function goToDetails() {
    if (!path.trim()) {
      setError("Enter a path to the app's directory first.");
      return;
    }
    setError(null);
    setStep(2);
  }

  function updateEnvRow(index: number, field: keyof EnvRow, value: string) {
    setEnvRows((rows) => rows.map((r, i) => (i === index ? { ...r, [field]: value } : r)));
  }

  function removeEnvRow(index: number) {
    setEnvRows((rows) => rows.filter((_, i) => i !== index));
  }

  async function handleSubmit() {
    setSubmitting(true);
    setError(null);
    try {
      const env: Record<string, string> = {};
      for (const row of envRows) {
        if (row.key.trim()) env[row.key.trim()] = row.value;
      }
      await api.addApp({
        path: path.trim(),
        name: name.trim() || null,
        runtime: runtime || null,
        command: command.trim() ? command.trim().split(/\s+/) : null,
        port: port.trim() ? Number(port.trim()) : null,
        domain: domain.trim() || null,
        path_prefix: pathPrefix.trim() || null,
        env,
        restart_policy: restartPolicy,
      });
      onAdded();
    } catch (e) {
      setError(String(e));
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          <h2>Add App</h2>
          <button className="icon-button" onClick={onClose} aria-label="Close">
            ×
          </button>
        </div>

        <div className="wizard-steps">
          <span className={step === 1 ? "step active" : "step"}>1. Locate</span>
          <span className={step === 2 ? "step active" : "step"}>2. Configure</span>
        </div>

        {error && <div className="error-banner">{error}</div>}

        {step === 1 && (
          <div className="modal-body">
            <label>
              App directory
              <div className="path-row">
                <input
                  value={path}
                  onChange={(e) => {
                    setPath(e.target.value);
                    setDetectedRuntime(null);
                  }}
                  placeholder="/home/user/my-app"
                />
                <button type="button" onClick={handleBrowse}>
                  Browse…
                </button>
              </div>
            </label>
            <button type="button" disabled={!path.trim() || detecting} onClick={handleDetect}>
              {detecting ? "Detecting…" : "Detect runtime"}
            </button>
            {detectedRuntime && (
              <p className="hint">Detected runtime: <strong>{detectedRuntime}</strong></p>
            )}
            {detectedRuntime === null && detecting === false && path.trim() && (
              <p className="hint muted">
                Runtime not yet detected — click "Detect runtime" or pick one manually on the next step.
              </p>
            )}
            <div className="modal-footer">
              <button type="button" onClick={onClose}>
                Cancel
              </button>
              <button type="button" className="primary" onClick={goToDetails}>
                Next
              </button>
            </div>
          </div>
        )}

        {step === 2 && (
          <div className="modal-body">
            <label>
              Name <span className="muted">(defaults to directory name)</span>
              <input value={name} onChange={(e) => setName(e.target.value)} placeholder="my-app" />
            </label>
            <label>
              Runtime
              <select value={runtime} onChange={(e) => setRuntime(e.target.value as RuntimeKind)}>
                <option value="">Auto-detect on add</option>
                {RUNTIMES.map((r) => (
                  <option key={r} value={r}>
                    {r}
                  </option>
                ))}
              </select>
            </label>
            <label>
              Command <span className="muted">(optional override, space-separated)</span>
              <input value={command} onChange={(e) => setCommand(e.target.value)} placeholder="python3 app.py" />
            </label>
            <div className="field-row">
              <label>
                Port
                <input
                  value={port}
                  onChange={(e) => setPort(e.target.value.replace(/[^0-9]/g, ""))}
                  placeholder="3000"
                />
              </label>
              <label>
                Domain
                <input value={domain} onChange={(e) => setDomain(e.target.value)} placeholder="app.example.com" />
              </label>
              <label>
                Path prefix
                <input value={pathPrefix} onChange={(e) => setPathPrefix(e.target.value)} placeholder="/api" />
              </label>
            </div>
            <label>
              Restart policy
              <select value={restartPolicy} onChange={(e) => setRestartPolicy(e.target.value as RestartPolicy)}>
                {RESTART_POLICIES.map((p) => (
                  <option key={p} value={p}>
                    {p}
                  </option>
                ))}
              </select>
            </label>

            <div className="env-editor">
              <div className="env-editor-header">
                <span>Environment variables</span>
                <button type="button" onClick={() => setEnvRows((rows) => [...rows, { key: "", value: "" }])}>
                  + Add
                </button>
              </div>
              {envRows.map((row, i) => (
                <div key={i} className="env-row">
                  <input
                    value={row.key}
                    onChange={(e) => updateEnvRow(i, "key", e.target.value)}
                    placeholder="KEY"
                  />
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

            <div className="modal-footer">
              <button type="button" onClick={() => setStep(1)}>
                Back
              </button>
              <button type="button" className="primary" disabled={submitting} onClick={handleSubmit}>
                {submitting ? "Adding…" : "Add app"}
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
