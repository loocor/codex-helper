import { installBridge } from "./bridge";
import { waitForCodexTarget } from "./cdp";
import { createLaunchTimer } from "./debug";
import { resolveDebugPortForLaunch } from "./debug-port";
import { ensureCodexLaunchedWithDebugPort } from "./launcher";
import { buildRuntimeScripts, resolveCodexAppPath } from "./paths";

type LaunchOptions = {
	appPath?: string;
	preferredDebugPort: number;
	explicitDebugPort?: number;
};

function parseArgs(args: string[]): LaunchOptions {
	const options: LaunchOptions = { preferredDebugPort: 9229 };
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
				throw new Error(`Invalid debug port: ${String(rawValue)}`);
			}
			options.explicitDebugPort = value;
			options.preferredDebugPort = value;
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
	console.log(
		`Usage: bun src/launch.ts [--app-path /Applications/Codex.app] [--debug-port <port>]`,
	);
	console.log("");
	console.log(
		"By default the launcher scans for an existing Codex CDP port or picks a free local port.",
	);
	console.log("Pass --debug-port only when you need a specific port.");
}

function ensureLocalCdpBypassesProxy(): void {
	const bypassHosts = ["127.0.0.1", "localhost"];
	const existing = (process.env.NO_PROXY ?? process.env.no_proxy ?? "")
		.split(",")
		.map((entry) => entry.trim())
		.filter(Boolean);
	process.env.NO_PROXY = [...new Set([...existing, ...bypassHosts])].join(",");
}

async function main(): Promise<void> {
	const options = parseArgs(Bun.argv.slice(2));
	ensureLocalCdpBypassesProxy();
	const appPath = resolveCodexAppPath(options.appPath);
	const timer = createLaunchTimer();
	const resolveOptions: Parameters<typeof resolveDebugPortForLaunch>[0] = {
		preferred: options.preferredDebugPort,
		timer,
	};
	if (options.explicitDebugPort !== undefined) {
		resolveOptions.explicitPort = options.explicitDebugPort;
	}
	const {
		port: debugPort,
		mode,
		portHold,
	} = await resolveDebugPortForLaunch(resolveOptions);
	process.env.CODEX_HELPER_DEBUG_PORT = String(debugPort);
	timer.stage("start", { app: appPath, port: debugPort, mode });
	await ensureCodexLaunchedWithDebugPort(
		appPath,
		debugPort,
		timer,
		mode,
		portHold,
	);
	const target = await waitForCodexTarget(debugPort, timer);
	const runtimeScripts = buildRuntimeScripts();
	const disconnect = await installBridge({
		debugPort,
		targetId: target.id,
		runtimeScripts,
		timer,
	});
	timer.stage("ready", { targetId: target.id, port: debugPort });
	console.log(
		`Codex Helper injected into target ${target.id} on debug port ${debugPort}`,
	);
	process.on("SIGINT", () => {
		disconnect();
		process.exit(0);
	});
	await new Promise(() => {});
}

main().catch((error) => {
	console.error(error instanceof Error ? error.message : String(error));
	process.exit(1);
});
