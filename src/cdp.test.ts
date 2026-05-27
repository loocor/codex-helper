import { expect, test } from "bun:test";

import {
	codexInjectablePageTargets,
	codexPageTargets,
	findCodexPageTarget,
	pickCodexPageTarget,
	type CdpTarget,
} from "./cdp";

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

test("codexPageTargets returns all injectable Codex pages", () => {
	const targets: CdpTarget[] = [
		target("one", "Codex", "app://-/index.html"),
		target("two", "Codex", "app://-/index.html"),
		target("settling", "app://-/index.html", "app://-/index.html"),
		{
			id: "worker",
			type: "worker",
			title: "Codex",
			url: "app://-/index.html",
			webSocketDebuggerUrl: "ws://worker",
		},
		{
			id: "missing-websocket",
			type: "page",
			title: "Codex",
			url: "app://-/index.html",
		},
	];

	expect(codexPageTargets(targets).map((target) => target.id)).toEqual([
		"one",
		"two",
		"settling",
	]);
});

test("codexInjectablePageTargets accepts browser target infos", () => {
	const targets: CdpTarget[] = [
		{
			id: "browser-page",
			type: "page",
			title: "app://-/index.html",
			url: "app://-/index.html",
		},
		{
			id: "browser-tab",
			type: "tab",
			title: "Codex",
			url: "app://-/index.html",
		},
		{
			id: "worker",
			type: "worker",
			title: "Codex",
			url: "app://-/index.html",
		},
	];

	expect(
		codexInjectablePageTargets(targets).map((target) => target.id),
	).toEqual(["browser-page"]);
});
