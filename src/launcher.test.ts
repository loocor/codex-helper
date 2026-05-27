import { expect, test } from "bun:test";

import {
	codexLaunchCommand,
	isKillablePortBlocker,
} from "./launcher";

test("isKillablePortBlocker allows Codex listeners only", () => {
	expect(
		isKillablePortBlocker(
			"/Applications/Codex.app/Contents/MacOS/Codex --remote-debugging-port=9229",
		),
	).toBe(true);
	expect(
		isKillablePortBlocker(
			"/System/Library/PrivateFrameworks/SkyComputerUseService",
		),
	).toBe(false);
	expect(isKillablePortBlocker("codex-helper --bridge")).toBe(false);
	expect(isKillablePortBlocker("codex --serve")).toBe(false);
});

test("codex launch command starts macOS app bundles through LaunchServices", () => {
	const command = codexLaunchCommand("/Applications/Codex.app", 9229, "darwin");

	expect(command.program).toBe("open");
	expect(command.args).toEqual([
		"-na",
		"/Applications/Codex.app",
		"--args",
		"--remote-debugging-port=9229",
		"--remote-debugging-address=127.0.0.1",
		"--remote-allow-origins=http://127.0.0.1:9229",
	]);
});

test("codex launch command starts executables directly off macOS", () => {
	const command = codexLaunchCommand("/opt/codex/codex", 9229, "linux");

	expect(command.program).toBe("/opt/codex/codex");
	expect(command.args).toEqual([
		"--remote-debugging-port=9229",
		"--remote-debugging-address=127.0.0.1",
		"--remote-allow-origins=http://127.0.0.1:9229",
	]);
});
