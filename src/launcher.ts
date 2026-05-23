import { spawn, spawnSync } from "node:child_process";

import { hasCodexCdpTarget, isDebugPortReady, waitForDebugPort } from "./cdp";
import type { LaunchTimer } from "./debug";
import type { PortHold } from "./debug-port";
import { describePortBlockers, listenPidsOnPort, processCommand } from "./port";

export function codexBinaryPath(appPath: string): string {
	return `${appPath}/Contents/MacOS/Codex`;
}

export function codexDebugArgs(debugPort: number): string[] {
	return [
		`--remote-debugging-port=${debugPort}`,
		`--remote-allow-origins=http://127.0.0.1:${debugPort}`,
	];
}

export function isCodexRunning(): boolean {
	const result = spawnSync("pgrep", ["-x", "Codex"], { stdio: "ignore" });
	return result.status === 0;
}

export async function quitCodex(
	timer: LaunchTimer,
	timeoutMs = 15000,
): Promise<void> {
	spawnSync("osascript", ["-e", 'tell application "Codex" to quit'], {
		stdio: "ignore",
	});
	const startedAt = Date.now();
	let lastProgressAt = startedAt;
	while (Date.now() - startedAt < timeoutMs) {
		if (!isCodexRunning()) {
			timer.stage("quit codex done", { waitedMs: Date.now() - startedAt });
			return;
		}
		const now = Date.now();
		if (now - lastProgressAt >= 2000) {
			timer.stage("quit codex waiting", { waitedMs: now - startedAt });
			lastProgressAt = now;
		}
		await Bun.sleep(250);
	}
	throw new Error("Timed out waiting for Codex to quit");
}

export function isKillablePortBlocker(command: string): boolean {
	const normalized = command.toLowerCase();
	return (
		normalized.includes("codex.app") ||
		/\bcodex\b/.test(normalized) ||
		normalized.includes("/codex.app/")
	);
}

async function terminatePid(
	pid: number,
	signal: "SIGTERM" | "SIGKILL",
): Promise<void> {
	await new Promise<void>((resolve) => {
		const child = spawn(
			"kill",
			[signal === "SIGKILL" ? "-9" : "", String(pid)].filter(Boolean),
			{
				stdio: "ignore",
			},
		);
		child.on("close", () => resolve());
		child.on("error", () => resolve());
	});
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
	const args = ["-na", appPath, "--args", ...codexDebugArgs(debugPort)];
	timer.stage("open codex", {
		app: appPath,
		port: debugPort,
		launchArgs: codexDebugArgs(debugPort).join(" "),
	});
	const child = spawn("open", args, { stdio: "ignore", detached: true });
	child.unref();
}

export async function ensureCodexLaunchedWithDebugPort(
	appPath: string,
	debugPort: number,
	timer: LaunchTimer,
	mode: "attach" | "launch" = "launch",
	portHold?: PortHold,
): Promise<void> {
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
		if (await isDebugPortReady(debugPort)) {
			if (await hasCodexCdpTarget(debugPort)) {
				timer.stage("debug port ready", { port: debugPort, path: "existing" });
				return;
			}
			throw new Error(
				`Debug port ${debugPort} exposes a browser CDP endpoint but not Codex. Stop the other app or omit --debug-port to auto-select another port.`,
			);
		}
		timer.stage("debug port not ready", { port: debugPort });

		await releaseBlockedDebugPort(debugPort, timer);

		const codexRunning = isCodexRunning();
		timer.stage("check codex process", { running: codexRunning });
		if (codexRunning) {
			timer.stage("quit codex start");
			await quitCodex(timer);
			await Bun.sleep(500);
			timer.stage("post-quit delay", { delayMs: 500 });
			await releaseBlockedDebugPort(debugPort, timer);
		}

		spawnCodexWithDebugPort(appPath, debugPort, timer);
		await waitForDebugPort(debugPort, timer);
	} finally {
		portHold?.release();
	}
}
