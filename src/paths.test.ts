import { expect, test } from "bun:test";

import { defaultCodexAppPath, resolveCodexAppPath } from "./paths";

test("default desktop app path is ChatGPT.app", () => {
	expect(defaultCodexAppPath).toBe("/Applications/ChatGPT.app");
});

test("resolveCodexAppPath fails with the requested path", () => {
	expect(() =>
		resolveCodexAppPath("/tmp/codex-helper-missing/ChatGPT.app"),
	).toThrow("Desktop app not found: /tmp/codex-helper-missing/ChatGPT.app");
});
