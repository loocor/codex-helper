import { expect, test } from "bun:test";

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
