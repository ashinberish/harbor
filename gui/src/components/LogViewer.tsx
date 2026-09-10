import { useEffect, useRef, useState } from "react";
import { api } from "../api";

interface Props {
  name: string;
  onClose: () => void;
}

const POLL_MS = 2000;
const LINES = 200;

export default function LogViewer({ name, onClose }: Props) {
  const [stdout, setStdout] = useState<string[]>([]);
  const [stderr, setStderr] = useState<string[]>([]);
  const [stream, setStream] = useState<"stdout" | "stderr">("stdout");
  const [error, setError] = useState<string | null>(null);
  const [autoScroll, setAutoScroll] = useState(true);
  const bodyRef = useRef<HTMLPreElement>(null);

  useEffect(() => {
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout>;

    async function poll() {
      try {
        const resp = await api.getLogs(name, LINES);
        if (cancelled) return;
        setStdout(resp.stdout);
        setStderr(resp.stderr);
        setError(null);
      } catch (e) {
        if (!cancelled) setError(String(e));
      } finally {
        if (!cancelled) timer = setTimeout(poll, POLL_MS);
      }
    }
    poll();
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [name]);

  useEffect(() => {
    if (autoScroll && bodyRef.current) {
      bodyRef.current.scrollTop = bodyRef.current.scrollHeight;
    }
  }, [stdout, stderr, stream, autoScroll]);

  const lines = stream === "stdout" ? stdout : stderr;

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal modal-wide" onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          <h2>Logs — {name}</h2>
          <button className="icon-button" onClick={onClose} aria-label="Close">
            ×
          </button>
        </div>
        <div className="log-tabs">
          <button className={stream === "stdout" ? "tab active" : "tab"} onClick={() => setStream("stdout")}>
            stdout
          </button>
          <button className={stream === "stderr" ? "tab active" : "tab"} onClick={() => setStream("stderr")}>
            stderr
          </button>
          <label className="autoscroll-toggle">
            <input type="checkbox" checked={autoScroll} onChange={(e) => setAutoScroll(e.target.checked)} />
            Auto-scroll
          </label>
        </div>
        {error && <div className="error-banner">{error}</div>}
        <pre className="log-body" ref={bodyRef}>
          {lines.length === 0 ? "(no output yet)" : lines.join("\n")}
        </pre>
      </div>
    </div>
  );
}
