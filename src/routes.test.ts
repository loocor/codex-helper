import { expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import {
	bridgeRequestTimeoutMessage,
	bridgeRequestTimeoutMs,
} from "./bridge";
import { handleBridgeRequest } from "./routes";

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

test("dev bridge rejects malformed settings files explicitly", async () => {
	const previous = process.env.CODEX_HELPER_HOME;
	const root = mkdtempSync(join(tmpdir(), "codex-helper-routes-"));
	try {
		process.env.CODEX_HELPER_HOME = root;
		mkdirSync(root, { recursive: true });
		writeFileSync(
			join(root, "config.json"),
			'{ "markdownExportEnabled": "yes" }',
			"utf8",
		);

		const result = await handleBridgeRequest("/settings/get", {});

		expect(result).toEqual({
			status: "failed",
			message: "Settings value for markdownExportEnabled must be a boolean",
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
				markdownExportEnabled: true,
				sessionMoveEnabled: false,
				portForwardingEnabled: false,
				portAutoForwardWeb: true,
				portSameLocalPort: true,
				autoRenameMenuEnabled: true,
				markdownFriendlyFilenameEnabled: true,
				autoNamingMinChars: 8,
				autoNamingMaxChars: 12,
			},
		});
	} finally {
		if (previous === undefined) delete process.env.CODEX_HELPER_HOME;
		else process.env.CODEX_HELPER_HOME = previous;
	}
});

test("dev bridge creates default settings with chat title regeneration enabled", async () => {
	const previous = process.env.CODEX_HELPER_HOME;
	const root = mkdtempSync(join(tmpdir(), "codex-helper-routes-"));
	try {
		process.env.CODEX_HELPER_HOME = root;

		const result = await handleBridgeRequest("/settings/get", {});

		expect(result).toMatchObject({
			status: "ok",
			settings: {
				autoRenameMenuEnabled: true,
			},
		});
	} finally {
		if (previous === undefined) delete process.env.CODEX_HELPER_HOME;
		else process.env.CODEX_HELPER_HOME = previous;
	}
});

test("dev bridge accepts auto naming settings", async () => {
	const previous = process.env.CODEX_HELPER_HOME;
	const root = mkdtempSync(join(tmpdir(), "codex-helper-routes-"));
	try {
		process.env.CODEX_HELPER_HOME = root;

		const result = await handleBridgeRequest("/settings/set", {
			autoRenameMenuEnabled: true,
			markdownFriendlyFilenameEnabled: false,
			autoNamingMinChars: 3,
			autoNamingMaxChars: 7,
		});

		expect(result).toMatchObject({
			status: "ok",
			settings: {
				autoRenameMenuEnabled: true,
				markdownFriendlyFilenameEnabled: false,
				autoNamingMinChars: 3,
				autoNamingMaxChars: 7,
			},
		});
	} finally {
		if (previous === undefined) delete process.env.CODEX_HELPER_HOME;
		else process.env.CODEX_HELPER_HOME = previous;
	}
});

test("dev bridge prefers canonical auto naming settings over legacy keys", async () => {
	const previous = process.env.CODEX_HELPER_HOME;
	const root = mkdtempSync(join(tmpdir(), "codex-helper-routes-"));
	try {
		process.env.CODEX_HELPER_HOME = root;
		mkdirSync(root, { recursive: true });
		writeFileSync(
			join(root, "config.json"),
			'{ "autoNamingMinWords": 12, "autoNamingMinChars": 3, "autoNamingMaxWords": 18, "autoNamingMaxChars": 7 }',
			"utf8",
		);

		const readResult = await handleBridgeRequest("/settings/get", {});
		const updateResult = await handleBridgeRequest("/settings/set", {
			autoNamingMinWords: 14,
			autoNamingMinChars: 4,
		});

		expect(readResult).toMatchObject({
			status: "ok",
			settings: {
				autoNamingMinChars: 3,
				autoNamingMaxChars: 7,
			},
		});
		expect(updateResult).toMatchObject({
			status: "ok",
			settings: {
				autoNamingMinChars: 4,
				autoNamingMaxChars: 7,
			},
		});
	} finally {
		if (previous === undefined) delete process.env.CODEX_HELPER_HOME;
		else process.env.CODEX_HELPER_HOME = previous;
	}
});

test("dev bridge uses longer friendly timeouts for naming routes", () => {
	expect(bridgeRequestTimeoutMs("/settings/get")).toBe(10000);
	expect(bridgeRequestTimeoutMs("/auto-rename-chat")).toBe(120000);
	expect(bridgeRequestTimeoutMs("/export-markdown")).toBe(120000);
	expect(bridgeRequestTimeoutMessage("/auto-rename-chat")).toContain(
		"Regenerate chat title is still running after 120s",
	);
	expect(bridgeRequestTimeoutMessage("/export-markdown")).toContain(
		"Markdown export is still running after 120s",
	);
});

test("dev bridge no longer exposes helper session delete lifecycle routes", async () => {
	for (const path of [
		"/delete",
		"/undo",
		"/backups/list",
		"/backups/restore",
		"/backups/reveal",
	]) {
		const result = await handleBridgeRequest(path, {});

		expect(result).toEqual({
			status: "failed",
			message: `Unknown Codex Helper bridge path: ${path}`,
		});
	}
});
