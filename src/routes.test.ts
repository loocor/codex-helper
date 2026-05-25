import { expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

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
