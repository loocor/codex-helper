import { spawn } from "node:child_process";

import { waitForCodexTarget } from "./cdp";
import { buildRuntimeBundle, injectRuntime } from "./bridge";
import { resolveCodexAppPath, runtimeScriptPath, zedOpenTweakPath } from "./paths";

type LaunchOptions = {
  appPath?: string;
  debugPort: number;
};

function parseArgs(args: string[]): LaunchOptions {
  const options: LaunchOptions = { debugPort: 9229 };
  for (let index = 0; index < args.length; index += 1) {
    const arg = args[index];
    if (arg === "--app-path") {
      const value = args[++index];
      if (!value) throw new Error("--app-path requires a value");
      options.appPath = value;
    } else if (arg === "--debug-port") {
      const rawValue = args[++index];
      if (!rawValue) throw new Error("--debug-port requires a value");
      const value = Number(rawValue);
      if (!Number.isInteger(value) || value <= 0 || value > 65535) {
        throw new Error(`Invalid debug port: ${String(args[index])}`);
      }
      options.debugPort = value;
    } else if (arg === "--help") {
      printHelp();
      process.exit(0);
    } else {
      throw new Error(`Unknown argument: ${arg}`);
    }
  }
  return options;
}

function printHelp(): void {
  console.log(`Usage: bun src/launch.ts [--app-path /Applications/Codex.app] [--debug-port 9229]`);
}

async function launchCodex(appPath: string, debugPort: number): Promise<void> {
  const child = spawn("open", [
    "-na",
    appPath,
    "--args",
    `--remote-debugging-port=${debugPort}`,
  ], {
    stdio: "ignore",
    detached: true,
  });
  child.unref();
}

async function main(): Promise<void> {
  const options = parseArgs(Bun.argv.slice(2));
  const appPath = resolveCodexAppPath(options.appPath);
  await launchCodex(appPath, options.debugPort);

  const target = await waitForCodexTarget(options.debugPort);
  const webSocketUrl = target.webSocketDebuggerUrl;
  if (!webSocketUrl) {
    throw new Error("Selected Codex CDP target has no websocket URL");
  }

  const bundle = buildRuntimeBundle([runtimeScriptPath(), zedOpenTweakPath()]);
  await injectRuntime(webSocketUrl, bundle);
  console.log(`Codex Helper injected into target ${target.id}`);
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
});
