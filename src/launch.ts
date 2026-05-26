import { installBridge } from "./bridge";
import { waitForCodexTargets } from "./cdp";
import { createLaunchTimer } from "./debug";
import { resolveDebugPortForLaunch } from "./debug-port";
import {
	disconnectInjectedTargets,
	startCodexTargetWatcher,
	syncInjectedTargetsForTargets,
	type InjectedTarget,
} from "./injection-sync";
import { ensureCodexLaunchedWithDebugPort } from "./launcher";
import { buildRuntimeScripts, resolveCodexAppPath } from "./paths";
import { stopPortForwards } from "./routes";

type LaunchOptions = {
	appPath?: string;
	preferredDebugPort: number;
	explicitDebugPort?: number;
};

let nextHelperInstanceId = 0;

function createHelperInstanceId(): string {
	nextHelperInstanceId += 1;
	return `dev-helper-${Date.now()}-${nextHelperInstanceId}`;
}

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
		"By default the launcher starts a Helper-managed Codex on a reserved random local port.",
	);
	console.log("Pass --debug-port only when you intentionally want a specific CDP port.");
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
	const targets = await waitForCodexTargets(debugPort, timer);
	const runtimeScripts = buildRuntimeScripts();
	const injectedTargets = new Map<string, InjectedTarget>();
	const initialSync = await syncInjectedTargetsForTargets({
		targets,
		injectedTargets,
		timer,
		installTarget: (target) =>
			installBridge({
				debugPort,
				targetId: target.id,
				helperInstanceId: createHelperInstanceId(),
				runtimeScripts,
				timer,
			}),
	});
	if (initialSync.failures.length > 0 || injectedTargets.size !== targets.length) {
		for (const injected of injectedTargets.values()) injected.disconnect();
		injectedTargets.clear();
		throw new Error(
			`Codex Helper failed to inject all ${targets.length} target(s): ${initialSync.failures.join("; ")}`,
		);
	}
	timer.stage("ready", {
		targetIds: targets.map((target) => target.id).join(","),
		injected: injectedTargets.size,
		failures: initialSync.failures.join("; "),
		port: debugPort,
	});
	console.log(
		`Codex Helper injected into ${injectedTargets.size} target(s) on debug port ${debugPort}`,
	);
	let cleanedUp = false;
	const cleanupManagedCodexResources = () => {
		stopPortForwards();
		disconnectInjectedTargets(injectedTargets);
	};
	const stopTargetWatcher = startCodexTargetWatcher({
		debugPort,
		runtimeScripts,
		injectedTargets,
		timer,
		createHelperInstanceId,
		onCodexDisconnected: cleanupManagedCodexResources,
		debugTargetEvents: process.env.CODEX_HELPER_DEBUG_TARGET_EVENTS === "1",
	});
	const cleanupOnce = () => {
		if (cleanedUp) return;
		cleanedUp = true;
		stopTargetWatcher();
		cleanupManagedCodexResources();
	};
	const exitAfterSignal = () => {
		cleanupOnce();
		process.exit(0);
	};
	process.on("SIGINT", exitAfterSignal);
	process.on("SIGTERM", exitAfterSignal);
	process.on("SIGHUP", exitAfterSignal);
	process.on("exit", cleanupOnce);
	await new Promise(() => {});
}

main().catch((error) => {
	console.error(error instanceof Error ? error.message : String(error));
	process.exit(1);
});
