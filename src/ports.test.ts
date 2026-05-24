import { expect, test } from "bun:test";
import { chmodSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawn as spawnProcess } from "node:child_process";
import { createServer } from "node:net";

import {
	PortForwardManager,
	buildSshArgs,
	discoveryRequestFromPayload,
	parseRemoteListeningPorts,
	requestedLocalPortAvailable,
	localPortAvailable,
	requestFromPayload,
} from "./ports";

function freePort(): Promise<number> {
	return new Promise((resolve, reject) => {
		const server = createServer();
		server.on("error", reject);
		server.listen(0, "127.0.0.1", () => {
			const address = server.address();
			if (!address || typeof address === "string") {
				server.close();
				reject(new Error("server did not expose a TCP address"));
				return;
			}
			const port = address.port;
			server.close(() => resolve(port));
		});
	});
}

async function waitForLocalPortAvailable(
	port: number,
	timeoutMs = 1500,
): Promise<boolean> {
	const deadline = Date.now() + timeoutMs;
	while (Date.now() < deadline) {
		if (await localPortAvailable(port)) return true;
		await new Promise((resolve) => setTimeout(resolve, 50));
	}
	return localPortAvailable(port);
}

test("requestFromPayload accepts valid manual requests", () => {
	const request = requestFromPayload({
		hostId: "remote-ssh-codex-managed:box",
		remotePath: "/srv/app",
		threadId: "thread-1",
		remotePort: 5173,
		localPort: 15173,
		source: "manual",
	});

	expect(request).toEqual({
		hostId: "remote-ssh-codex-managed:box",
		remotePath: "/srv/app",
		threadId: "thread-1",
		remotePort: 5173,
		localPort: 15173,
		source: "manual",
	});
});

test("requestFromPayload accepts zero as automatic local port", () => {
	const request = requestFromPayload({
		hostId: "remote-ssh-codex-managed:box",
		remotePath: "/srv/app",
		threadId: "thread-1",
		remotePort: 5173,
		localPort: 0,
		source: "auto",
	});

	expect(request.localPort).toBe(0);
});

test("requestFromPayload rejects missing host id", () => {
	expect(() =>
		requestFromPayload({
			remotePort: 5173,
			localPort: 5173,
			source: "auto",
		}),
	).toThrow("Remote host id is required");
});

test("requestFromPayload requires a scoped remote path and thread", () => {
	expect(() =>
		requestFromPayload({
			hostId: "remote-ssh-codex-managed:box",
			remotePort: 5173,
			localPort: 5173,
			source: "auto",
		}),
	).toThrow("Remote path is required");
	expect(() =>
		requestFromPayload({
			hostId: "remote-ssh-codex-managed:box",
			remotePath: "/srv/app",
			remotePort: 5173,
			localPort: 5173,
			source: "auto",
		}),
	).toThrow("Thread id is required");
});

test("discoveryRequestFromPayload requires a scoped remote path and thread", () => {
	expect(() =>
		discoveryRequestFromPayload({
			hostId: "remote-ssh-codex-managed:box",
		}),
	).toThrow("Remote path is required");
	expect(() =>
		discoveryRequestFromPayload({
			hostId: "remote-ssh-codex-managed:box",
			remotePath: "/srv/app",
		}),
	).toThrow("Thread id is required");
});

test("localPortAvailable reports bound ports", async () => {
	const server = createServer();
	await new Promise<void>((resolve, reject) => {
		server.on("error", reject);
		server.listen(0, "127.0.0.1", () => resolve());
	});
	const address = server.address();
	if (!address || typeof address === "string") {
		throw new Error("server did not expose a TCP address");
	}

	expect(await localPortAvailable(address.port)).toBe(false);

	await new Promise<void>((resolve) => server.close(() => resolve()));
	expect(await localPortAvailable(address.port)).toBe(true);
});

test("requestedLocalPortAvailable treats zero as automatic allocation", async () => {
	expect(await requestedLocalPortAvailable(0)).toBe(false);
});

test("parseRemoteListeningPorts includes custom workspace ports", () => {
	const ports = parseRemoteListeningPorts(
		[
			"123\tvite\t/Volumes/External/GitHub/CodexHelper\t127.0.0.1:5173",
			"456\tpython\t/Volumes/External/GitHub/CodexHelper/api\t*:8000",
			"789\tpostgres\t/usr/local/var\t127.0.0.1:5432",
		].join("\n"),
		"/Volumes/External/GitHub/CodexHelper",
	);

	expect(ports).toEqual([
		{ remotePort: 5173, pid: 123, command: "vite" },
		{ remotePort: 8000, pid: 456, command: "python" },
	]);
});

test("parseRemoteListeningPorts removes ports that left the workspace", () => {
	expect(
		parseRemoteListeningPorts(
			"789\tpostgres\t/usr/local/var\t127.0.0.1:5432",
			"/Volumes/External/GitHub/CodexHelper",
		),
	).toEqual([]);
});

test("parseRemoteListeningPorts rejects root workspace expansion", () => {
	expect(
		parseRemoteListeningPorts(
			"123\tvite\t/Volumes/External/GitHub/CodexHelper\t127.0.0.1:5173",
			"/",
		),
	).toEqual([]);
});

test("buildSshArgs separates ssh options from target", () => {
	const args = buildSshArgs(
		{
			hostId: "remote-ssh-codex-managed:box",
			remotePath: "/srv/app",
			threadId: "thread-1",
			remotePort: 5173,
			localPort: 15173,
			source: "manual",
		},
		{ user: "", host: "-oProxyCommand=bad", port: null },
	);

	expect(args.at(-2)).toBe("--");
	expect(args.at(-1)).toBe("-oProxyCommand=bad");
});

test("buildSshArgs fails fast and keeps the tunnel alive predictably", () => {
	const args = buildSshArgs(
		{
			hostId: "remote-ssh-codex-managed:box",
			remotePath: "/srv/app",
			threadId: "thread-1",
			remotePort: 5173,
			localPort: 15173,
			source: "manual",
		},
		{ user: "", host: "box", port: null },
	);

	expect(args).toContain("ExitOnForwardFailure=yes");
	expect(args).toContain("BatchMode=yes");
	expect(args).toContain("ControlMaster=no");
	expect(args).toContain("ControlPath=none");
	expect(args).toContain("ServerAliveInterval=15");
	expect(args).toContain("ServerAliveCountMax=4");
});

test("PortForwardManager rejects tunnels when ssh exits immediately", async () => {
	const manager = new PortForwardManager({ sshProgram: "/usr/bin/false" });
	const localPort = await freePort();

	const result = await manager.start(
			{
				hostId: "remote-ssh-codex-managed:box",
				remotePath: "/srv/app",
				threadId: "thread-1",
				remotePort: 5173,
				localPort,
				source: "manual",
		},
		{ user: "", host: "example.invalid", port: null },
	);

	expect(result.status).toBe("failed");
	expect(await manager.list()).toEqual({ status: "ok", ports: [] });
});

test("PortForwardManager reports missing ssh program", async () => {
	const manager = new PortForwardManager({
		sshProgram: "/tmp/codex-helper-missing-ssh",
		startTimeoutMs: 100,
	});
	const localPort = await freePort();

	const result = await manager.start(
			{
				hostId: "remote-ssh-codex-managed:box",
				remotePath: "/srv/app",
				threadId: "thread-1",
				remotePort: 5173,
				localPort,
				source: "manual",
		},
		{ user: "", host: "example.invalid", port: null },
	);

	expect(result.status).toBe("failed");
	expect(String(result.message)).toContain("ENOENT");
	expect(await manager.list()).toEqual({ status: "ok", ports: [] });
});

test("PortForwardManager stop terminates stubborn tunnel children", async () => {
	const tempDir = mkdtempSync(join(tmpdir(), "codex-helper-ports-"));
	const pidPath = join(tempDir, "fake-ssh.pid");
	const scriptPath = join(tempDir, "fake-ssh.sh");
	writeFileSync(
		scriptPath,
		`#!/bin/sh
local_spec=""
while [ "$#" -gt 0 ]; do
  if [ "$1" = "-L" ]; then
    shift
    local_spec="$1"
  fi
  shift
done
local_port=$(printf "%s" "$local_spec" | awk -F: '{print $2}')
echo $$ > ${JSON.stringify(pidPath)}
exec python3 - "$local_port" <<'PY'
import signal
import socket
import sys
import time

signal.signal(signal.SIGTERM, signal.SIG_IGN)
sock = socket.socket()
sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
sock.bind(("127.0.0.1", int(sys.argv[1])))
sock.listen(1)
while True:
    time.sleep(1)
PY
`,
	);
	chmodSync(scriptPath, 0o755);
	const manager = new PortForwardManager({
		sshProgram: scriptPath,
		startTimeoutMs: 5000,
	});
	const localPort = await freePort();
	let childPid = 0;

	try {
		const result = await manager.start(
			{
				hostId: "remote-ssh-codex-managed:box",
				remotePath: "/srv/app",
				threadId: "thread-1",
				remotePort: 5173,
				localPort,
				source: "manual",
			},
			{ user: "", host: "example.invalid", port: null },
		);
		expect(result.status).toBe("ok");
		childPid = Number(readFileSync(pidPath, "utf8"));

		await manager.stop(String(result.id));

		expect(await waitForLocalPortAvailable(localPort)).toBe(true);
	} finally {
		if (childPid > 0) {
			await new Promise<void>((resolve) => {
				const child = spawnProcess("kill", ["-KILL", String(childPid)], {
					stdio: "ignore",
				});
				child.once("exit", () => resolve());
				child.once("error", () => resolve());
			});
		}
	}
});
