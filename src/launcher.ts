import { spawn, spawnSync } from "node:child_process";

import { hasCodexCdpTarget, isDebugPortReady, waitForDebugPort } from "./cdp";
import type { LaunchTimer } from "./debug";
import type { PortHold } from "./debug-port";
import { describePortBlockers, listenPidsOnPort, processCommand } from "./port";

export function codexBinaryPath(appPath: string): string {
	if (appPath.endsWith(".app")) return `${appPath}/Contents/MacOS/Codex`;
	return appPath;
}

export function codexDebugArgs(debugPort: number): string[] {
	return [
		`--remote-debugging-port=${debugPort}`,
		"--remote-debugging-address=127.0.0.1",
		`--remote-allow-origins=http://127.0.0.1:${debugPort}`,
	];
}

export function codexLaunchCommand(
	appPath: string,
	debugPort: number,
	platform = process.platform,
): { program: string; args: string[] } {
	const debugArgs = codexDebugArgs(debugPort);
	if (platform === "darwin" && appPath.endsWith(".app")) {
		return {
			program: "open",
			args: ["-na", appPath, "--args", ...debugArgs],
		};
	}
	return {
		program: codexBinaryPath(appPath),
		args: debugArgs,
	};
}

export function isKillablePortBlocker(command: string): boolean {
	const normalized = command.toLowerCase();
	return (
		normalized.includes("codex.app") ||
		normalized.includes("/codex.app/") ||
		normalized.includes("/contents/macos/codex")
	);
}

function terminatePid(pid: number, signal: "SIGTERM" | "SIGKILL"): void {
	spawnSync(
		"kill",
		[signal === "SIGKILL" ? "-9" : "", String(pid)].filter(Boolean),
		{
			stdio: "ignore",
		},
	);
}

export async function releaseBlockedDebugPort(
	debugPort: number,
	timer: LaunchTimer,
): Promise<void> {
	if (await isDebugPortReady(debugPort)) return;

	const initialPids = listenPidsOnPort(debugPort);
	if (initialPids.length === 0) return;

	const killablePids = initialPids.filter((pid) =>
		isKillablePortBlocker(processCommand(pid)),
	);
	const blockedBy = initialPids
		.filter((pid) => !killablePids.includes(pid))
		.map((pid) => `${pid}:${processCommand(pid)}`);

	if (blockedBy.length > 0) {
		throw new Error(
			`Debug port ${debugPort} is blocked by a non-Codex process: ${blockedBy.join("; ")}. Stop it manually or omit --debug-port to auto-select another port.`,
		);
	}

	const processes = killablePids
		.map((pid) => `${pid}:${processCommand(pid)}`)
		.join(" | ");
	timer.stage("debug port blocked", {
		port: debugPort,
		pids: killablePids.join(","),
		processes: processes.slice(0, 240),
	});

	for (const pid of killablePids) {
		await terminatePid(pid, "SIGTERM");
	}
	await Bun.sleep(750);

	if (await isDebugPortReady(debugPort)) {
		timer.stage("debug port released", { port: debugPort });
		return;
	}

	const remainingPids = listenPidsOnPort(debugPort).filter((pid) =>
		isKillablePortBlocker(processCommand(pid)),
	);
	for (const pid of remainingPids) {
		await terminatePid(pid, "SIGKILL");
	}
	if (remainingPids.length > 0) {
		await Bun.sleep(500);
		timer.stage("debug port force released", {
			port: debugPort,
			pids: remainingPids.join(","),
		});
	}

	if (listenPidsOnPort(debugPort).length > 0) {
		throw new Error(
			`Debug port ${debugPort} is still blocked after releasing Codex listeners: ${describePortBlockers(debugPort)}`,
		);
	}
}

export function spawnCodexWithDebugPort(
	appPath: string,
	debugPort: number,
	timer: LaunchTimer,
): void {
	const command = codexLaunchCommand(appPath, debugPort);
	timer.stage("open codex", {
		app: appPath,
		port: debugPort,
		launchArgs: command.args.join(" "),
	});
	const child = spawn(command.program, command.args, {
		stdio: "ignore",
		detached: true,
	});
	child.unref();
}

export async function ensureCodexLaunchedWithDebugPort(
	appPath: string,
	debugPort: number,
	timer: LaunchTimer,
	mode: "attach" | "launch" = "launch",
	portHold?: PortHold,
): Promise<void> {
	let heldPort = portHold;
	const releaseHeldPort = () => {
		heldPort?.release();
		heldPort = undefined;
	};
	try {
		timer.stage("probe debug port", { port: debugPort, mode });
		if (mode === "attach") {
			if (await hasCodexCdpTarget(debugPort)) {
				timer.stage("debug port ready", { port: debugPort, path: "existing" });
				return;
			}
			throw new Error(
				`Codex CDP is not ready on port ${debugPort}. Start Codex with remote debugging on that port or omit --debug-port to auto-select.`,
			);
		}
		if (!heldPort) {
			if (await isDebugPortReady(debugPort)) {
				if (await hasCodexCdpTarget(debugPort)) {
					throw new Error(
						`Debug port ${debugPort} already exposes Codex CDP. Pass --debug-port ${debugPort} only when you intend to attach to that existing Codex, or omit --debug-port to launch a Helper-managed Codex on a random port.`,
					);
				}
				throw new Error(
					`Debug port ${debugPort} exposes a browser CDP endpoint but not Codex. Stop the other app or omit --debug-port to auto-select another port.`,
				);
			}
		}
		timer.stage("debug port not ready", { port: debugPort });

		if (!heldPort) {
			await releaseBlockedDebugPort(debugPort, timer);
		}

		releaseHeldPort();
		spawnCodexWithDebugPort(appPath, debugPort, timer);
		await waitForDebugPort(debugPort, timer);
	} finally {
		releaseHeldPort();
	}
}
