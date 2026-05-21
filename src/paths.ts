import { existsSync } from "node:fs";

export const defaultCodexAppPath = "/Applications/Codex.app";

export function resolveCodexAppPath(explicitPath?: string): string {
  const candidate = explicitPath?.trim() || defaultCodexAppPath;
  if (!existsSync(candidate)) {
    throw new Error(`Codex app not found: ${candidate}`);
  }
  return candidate;
}

export function runtimeScriptPath(): string {
  return new URL("../runtime/renderer.js", import.meta.url).pathname;
}

export function zedOpenTweakPath(): string {
  return new URL("../runtime/tweaks/zed-open.js", import.meta.url).pathname;
}
