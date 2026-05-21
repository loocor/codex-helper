import { readFileSync } from "node:fs";

import { cdpCommand } from "./cdp";

export function buildBridgeScript(): string {
  return `
(() => {
  window.__codexHelperBridge = async (path, payload = {}) => {
    window.dispatchEvent(new CustomEvent("codex-helper:bridge", { detail: { path, payload } }));
    return { status: "ok", path };
  };
})();
`;
}

export function buildRuntimeBundle(paths: string[]): string {
  return [
    buildBridgeScript(),
    ...paths.map((path) => readFileSync(path, "utf8")),
  ].join("\n;\n");
}

export async function injectRuntime(webSocketUrl: string, source: string): Promise<void> {
  await cdpCommand(webSocketUrl, "Page.addScriptToEvaluateOnNewDocument", { source });
  await cdpCommand(webSocketUrl, "Runtime.evaluate", {
    expression: source,
    awaitPromise: false,
    allowUnsafeEvalBlockedByCSP: true,
  });
}
