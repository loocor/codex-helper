import { expect, test } from "bun:test";

import { findCodexPageTarget, pickCodexPageTarget, type CdpTarget } from "./cdp";

function target(id: string, title: string, url: string): CdpTarget {
	return {
		id,
		type: "page",
		title,
		url,
		webSocketDebuggerUrl: `ws://${id}`,
	};
}

test("findCodexPageTarget rejects non-Codex page targets", () => {
	expect(
		findCodexPageTarget([
			target("chrome", "Chrome Settings", "chrome://settings"),
			target("app", "Other App", "https://example.test"),
		]),
	).toBeNull();
});

test("pickCodexPageTarget can still fall back after launch owns the port", () => {
	expect(
		pickCodexPageTarget([target("launched", "", "https://example.test")]).id,
	).toBe("launched");
});
