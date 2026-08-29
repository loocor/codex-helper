import { expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import {
	bridgeRequestTimeoutMessage,
	bridgeRequestTimeoutMs,
} from "./bridge";
import { devtoolsUrlForTargetId, handleBridgeRequest } from "./routes";

test("dev bridge exposes port forwarding list route", async () => {
	const result = await handleBridgeRequest("/ports/list", {});

	expect(result).toEqual({ status: "ok", ports: [] });
});

test("dev bridge validates port forwarding requests", async () => {
	const result = await handleBridgeRequest("/ports/forward", {
		remotePort: 5173,
		localPort: 5173,
		source: "manual",
	});

	expect(result).toEqual({
		status: "failed",
		message: "Remote host id is required",
	});
});

test("dev bridge validates port discovery requests", async () => {
	const result = await handleBridgeRequest("/ports/discover", {
		hostId: "remote-ssh-codex-managed:box",
		threadId: "thread-1",
	});

	expect(result).toEqual({
		status: "failed",
		message: "Remote path is required",
	});
});

test("dev bridge only opens local forwarded urls externally", async () => {
	const result = await handleBridgeRequest("/url/open-external", {
		url: "https://example.com:3000",
	});

	expect(result).toEqual({
		status: "failed",
		message: "Only local forwarded URLs can be opened",
	});
});

test("devtools url uses caller target id", () => {
	const url = devtoolsUrlForTargetId(9229, "caller", [
		{
			id: "first",
			webSocketDebuggerUrl: "ws://127.0.0.1:9229/devtools/page/first",
		},
		{
			id: "caller",
			webSocketDebuggerUrl: "ws://127.0.0.1:9229/devtools/page/caller",
		},
	]);

	expect(url).toBe(
		"http://127.0.0.1:9229/devtools/inspector.html?ws=127.0.0.1:9229/devtools/page/caller",
	);
});

test("dev bridge returns helper directory paths for native settings", async () => {
	const previous = process.env.CODEX_HELPER_HOME;
	const root = mkdtempSync(join(tmpdir(), "codex-helper-routes-"));
	try {
		process.env.CODEX_HELPER_HOME = root;
		mkdirSync(join(root, "scripts"), { recursive: true });
		writeFileSync(join(root, "scripts", "custom.js"), "", "utf8");

		const scripts = await handleBridgeRequest("/runtime/user-scripts", {});
		const log = await handleBridgeRequest("/diagnostics/read-latest", {});

		expect(scripts).toEqual({
			status: "ok",
			path: join(root, "scripts"),
			scripts: ["custom.js"],
		});
		expect(log).toMatchObject({
			status: "ok",
			path: join(root, "logs", "codex-helper.jsonl"),
			contents: "",
		});
	} finally {
		if (previous === undefined) delete process.env.CODEX_HELPER_HOME;
		else process.env.CODEX_HELPER_HOME = previous;
	}
});

test("dev bridge does not log routine runtime activity", async () => {
	const previous = process.env.CODEX_HELPER_HOME;
	const root = mkdtempSync(join(tmpdir(), "codex-helper-routes-"));
	try {
		process.env.CODEX_HELPER_HOME = root;

		const result = await handleBridgeRequest(
			"/runtime/activity",
			{},
			{
				targetId: "target-1",
				helperInstanceId: "helper-1",
				href: "app://-/index.html",
				hasFocus: true,
				visibilityState: "visible",
			},
		);
		const log = await handleBridgeRequest("/diagnostics/read-latest", {});

		expect(result).toEqual({ status: "ok" });
		expect(log).toMatchObject({
			status: "ok",
			path: join(root, "logs", "codex-helper.jsonl"),
			contents: "",
		});
	} finally {
		if (previous === undefined) delete process.env.CODEX_HELPER_HOME;
		else process.env.CODEX_HELPER_HOME = previous;
	}
});

test("dev bridge rejects malformed settings files explicitly", async () => {
	const previous = process.env.CODEX_HELPER_HOME;
	const root = mkdtempSync(join(tmpdir(), "codex-helper-routes-"));
	try {
		process.env.CODEX_HELPER_HOME = root;
		mkdirSync(root, { recursive: true });
		writeFileSync(
			join(root, "config.json"),
			'{ "portForwardingEnabled": "yes" }',
			"utf8",
		);

		const result = await handleBridgeRequest("/settings/get", {});

		expect(result).toEqual({
			status: "failed",
			message: "Settings value for portForwardingEnabled must be a boolean",
		});
	} finally {
		if (previous === undefined) delete process.env.CODEX_HELPER_HOME;
		else process.env.CODEX_HELPER_HOME = previous;
	}
});

test("dev bridge accepts settings with known removed keys", async () => {
	const previous = process.env.CODEX_HELPER_HOME;
	const root = mkdtempSync(join(tmpdir(), "codex-helper-routes-"));
	try {
		process.env.CODEX_HELPER_HOME = root;
		mkdirSync(root, { recursive: true });
		writeFileSync(
			join(root, "config.json"),
			`{
  "markdownExportEnabled": true,
  "sessionDeleteEnabled": true,
  "autoRenameMenuEnabled": true,
  "markdownFriendlyFilenameEnabled": true,
  "autoNamingMinChars": 8,
  "autoNamingMaxChars": 12
}`,
			"utf8",
		);

		const result = await handleBridgeRequest("/settings/get", {});

		expect(result).toEqual({
			status: "ok",
			settings: {
				portForwardingEnabled: false,
				portAutoForwardWeb: true,
				portSameLocalPort: true,
				hideUsageLimitBannerEnabled: false,
				launchAtLoginEnabled: false,
			},
		});
	} finally {
		if (previous === undefined) delete process.env.CODEX_HELPER_HOME;
		else process.env.CODEX_HELPER_HOME = previous;
	}
});

test("dev bridge accepts usage-limit overlay hide setting", async () => {
	const previous = process.env.CODEX_HELPER_HOME;
	const root = mkdtempSync(join(tmpdir(), "codex-helper-routes-"));
	try {
		process.env.CODEX_HELPER_HOME = root;

		const result = await handleBridgeRequest("/settings/set", {
			hideUsageLimitBannerEnabled: true,
		});

		expect(result).toMatchObject({
			status: "ok",
			settings: {
				hideUsageLimitBannerEnabled: true,
			},
		});
	} finally {
		if (previous === undefined) delete process.env.CODEX_HELPER_HOME;
		else process.env.CODEX_HELPER_HOME = previous;
	}
});

test("dev bridge accepts launch at login setting", async () => {
	const previous = process.env.CODEX_HELPER_HOME;
	const root = mkdtempSync(join(tmpdir(), "codex-helper-routes-"));
	try {
		process.env.CODEX_HELPER_HOME = root;

		const result = await handleBridgeRequest("/settings/set", {
			launchAtLoginEnabled: true,
		});

		expect(result).toMatchObject({
			status: "ok",
			settings: {
				launchAtLoginEnabled: true,
			},
		});
	} finally {
		if (previous === undefined) delete process.env.CODEX_HELPER_HOME;
		else process.env.CODEX_HELPER_HOME = previous;
	}
});


test("dev bridge creates default settings without chat title regeneration", async () => {
	const previous = process.env.CODEX_HELPER_HOME;
	const root = mkdtempSync(join(tmpdir(), "codex-helper-routes-"));
	try {
		process.env.CODEX_HELPER_HOME = root;

		const result = await handleBridgeRequest("/settings/get", {});

		expect(result).toMatchObject({
			status: "ok",
			settings: {
				portForwardingEnabled: false,
				hideUsageLimitBannerEnabled: false,
			},
		});
		expect(result).not.toHaveProperty("settings.autoRenameMenuEnabled");
		expect(JSON.stringify(result)).not.toContain("autoRenameMenuEnabled");
		expect(JSON.stringify(result)).not.toContain("markdownExportEnabled");
		expect(JSON.stringify(result)).not.toContain("sessionMoveEnabled");
	} finally {
		if (previous === undefined) delete process.env.CODEX_HELPER_HOME;
		else process.env.CODEX_HELPER_HOME = previous;
	}
});

test("dev bridge rejects updates to removed session settings", async () => {
	const previous = process.env.CODEX_HELPER_HOME;
	const root = mkdtempSync(join(tmpdir(), "codex-helper-routes-"));
	try {
		process.env.CODEX_HELPER_HOME = root;

		const result = await handleBridgeRequest("/settings/set", {
			markdownExportEnabled: true,
		});

		expect(result).toEqual({
			status: "failed",
			message: "Unknown settings key: markdownExportEnabled",
		});
	} finally {
		if (previous === undefined) delete process.env.CODEX_HELPER_HOME;
		else process.env.CODEX_HELPER_HOME = previous;
	}
});

test("dev bridge uses the default timeout for remaining routes", () => {
	expect(bridgeRequestTimeoutMs("/settings/get")).toBe(10000);
	expect(bridgeRequestTimeoutMs("/export-markdown")).toBe(10000);
	expect(bridgeRequestTimeoutMessage("/export-markdown")).toContain(
		"timed out after 10000ms",
	);
});

test("dev bridge no longer exposes helper session delete lifecycle routes", async () => {
	for (const path of [
		"/delete",
		"/undo",
		"/backups/list",
		"/backups/restore",
		"/backups/reveal",
		"/export-markdown",
		"/fork-thread-project",
		"/projects/remote-list",
		"/zed-remote/open",
		"/zed-remote/status",
	]) {
		const result = await handleBridgeRequest(path, {});

		expect(result).toEqual({
			status: "failed",
			message: `Unknown Codex Helper bridge path: ${path}`,
		});
	}
});
