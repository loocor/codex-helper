import { spawn, spawnSync } from "node:child_process";
import { join } from "node:path";

import { isDebugPortReady } from "./cdp";
import type { LaunchTimer } from "./debug";

export function codexBinaryPath(appPath: string): string {
	return join(appPath, "Contents/MacOS/Codex");
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

export async function ensureCodexLaunchedWithDebugPort(
	appPath: string,
	debugPort: number,
	timer: LaunchTimer,
): Promise<void> {
	timer.stage("probe debug port", { port: debugPort });
	if (await isDebugPortReady(debugPort)) {
		timer.stage("debug port ready", { port: debugPort, path: "existing" });
		return;
	}
	timer.stage("debug port not ready", { port: debugPort });

	const codexRunning = isCodexRunning();
	timer.stage("check codex process", { running: codexRunning });
	if (codexRunning) {
		timer.stage("quit codex start");
		await quitCodex(timer);
		await Bun.sleep(500);
		timer.stage("post-quit delay", { delayMs: 500 });
	}

	const binaryPath = codexBinaryPath(appPath);
	const args = codexDebugArgs(debugPort);
	timer.stage("spawn codex", { binary: binaryPath, port: debugPort });
	const child = spawn(binaryPath, args, { stdio: "ignore", detached: true });
	child.unref();
	await Bun.sleep(1000);
	timer.stage("post-spawn delay", { delayMs: 1000 });
}
