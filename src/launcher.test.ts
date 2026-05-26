import { expect, test } from "bun:test";

import { isKillablePortBlocker, parsePidList } from "./launcher";

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

test("parsePidList keeps valid pids only", () => {
	expect(parsePidList("123\nabc\n0\n456\n")).toEqual([123, 456]);
});
