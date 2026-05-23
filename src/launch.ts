import { installBridge, loadRuntimeScripts } from "./bridge";
import { waitForCodexTarget } from "./cdp";
import { createLaunchTimer } from "./debug";
import { ensureCodexLaunchedWithDebugPort } from "./launcher";
import {
	resolveCodexAppPath,
	runtimeScriptPath,
	zedOpenTweakPath,
} from "./paths";

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
	console.log(
		`Usage: bun src/launch.ts [--app-path /Applications/Codex.app] [--debug-port 9229]`,
	);
}

async function main(): Promise<void> {
	const options = parseArgs(Bun.argv.slice(2));
	const appPath = resolveCodexAppPath(options.appPath);
	const timer = createLaunchTimer();
	process.env.CODEX_HELPER_DEBUG_PORT = String(options.debugPort);
	timer.stage("start", { app: appPath, port: options.debugPort });
	await ensureCodexLaunchedWithDebugPort(appPath, options.debugPort, timer);
	const target = await waitForCodexTarget(options.debugPort, timer);
	const runtimeScripts = loadRuntimeScripts([
		runtimeScriptPath(),
		zedOpenTweakPath(),
	]);
	const disconnect = await installBridge({
		debugPort: options.debugPort,
		targetId: target.id,
		runtimeScripts,
		timer,
	});
	timer.stage("ready", { targetId: target.id });
	console.log(`Codex Helper injected into target ${target.id}`);
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
