import type { AppState } from "../api";

export default function StatusBadge({ state }: { state: AppState }) {
  return <span className={`badge badge-${state}`}>{state}</span>;
}
