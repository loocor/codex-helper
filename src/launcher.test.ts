import { expect, test } from "bun:test";

import { isKillablePortBlocker } from "./launcher";

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
});
