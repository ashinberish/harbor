import { invoke } from "@tauri-apps/api/core";

export type RuntimeKind = "python" | "node" | "dotnet" | "java" | "rust" | "custom";
export type RestartPolicy = "never" | "on-failure" | "always";
export type AppState = "stopped" | "running" | "restarting" | "crashed";

export interface AppStatus {
  name: string;
  state: AppState;
  pid: number | null;
  restart_count: number;
  uptime_seconds: number | null;
  cpu_percent: number | null;
  memory_bytes: number | null;
  runtime: RuntimeKind;
  port: number | null;
  domain: string | null;
  path_prefix: string | null;
  last_exit_code: number | null;
}

export interface AppConfig {
  name: string;
  path: string;
  runtime: RuntimeKind;
  command: string[];
  working_dir: string | null;
  port: number | null;
  domain: string | null;
  path_prefix: string | null;
  env: Record<string, string>;
  restart_policy: RestartPolicy;
  restart_backoff_seconds: number;
  restart_max_backoff_seconds: number;
}

export interface AddAppRequest {
  path: string;
  name?: string | null;
  runtime?: RuntimeKind | null;
  command?: string[] | null;
  port?: number | null;
  domain?: string | null;
  path_prefix?: string | null;
  env: Record<string, string>;
  restart_policy?: RestartPolicy | null;
}

export interface LogsResponse {
  name: string;
  stdout: string[];
  stderr: string[];
}

export interface ReloadResponse {
  apps_loaded: number;
}

/** Wraps every daemon-backed Tauri command with the same error shape: the
 * Rust side always rejects with a plain string message (see
 * `gui/src-tauri/src/daemon.rs`), so callers can just catch and display it. */
export const api = {
  listApps: () => invoke<AppStatus[]>("list_apps"),
  getApp: (name: string) => invoke<AppStatus>("get_app", { name }),
  addApp: (req: AddAppRequest) => invoke<AppStatus>("add_app", { req }),
  startApp: (name: string) => invoke<void>("start_app", { name }),
  stopApp: (name: string) => invoke<void>("stop_app", { name }),
  restartApp: (name: string) => invoke<void>("restart_app", { name }),
  removeApp: (name: string) => invoke<void>("remove_app", { name }),
  apply: () => invoke<ReloadResponse>("apply"),
  getLogs: (name: string, lines: number) => invoke<LogsResponse>("get_logs", { name, lines }),
  getAppConfig: (name: string) => invoke<AppConfig>("get_app_config", { name }),
  updateAppConfig: (config: AppConfig) => invoke<AppStatus>("update_app_config", { config }),
  detectRuntime: (path: string) => invoke<RuntimeKind | null>("detect_runtime", { path }),
};

export function formatBytes(bytes: number | null): string {
  if (bytes === null) return "—";
  const units = ["B", "KiB", "MiB", "GiB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return unit === 0 ? `${value.toFixed(0)}${units[unit]}` : `${value.toFixed(1)}${units[unit]}`;
}

export function formatUptime(seconds: number | null): string {
  if (seconds === null) return "—";
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = Math.floor(seconds % 60);
  if (h > 0) return `${h}h ${m}m`;
  if (m > 0) return `${m}m ${s}s`;
  return `${s}s`;
}
