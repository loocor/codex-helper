import { spawn, type ChildProcessByStdio } from "node:child_process";
import { createServer } from "node:net";
import type { Readable } from "node:stream";

import type { SshTarget } from "./zed";

type JsonValue =
	| null
	| boolean
	| number
	| string
	| JsonValue[]
	| { [key: string]: JsonValue };

type PortForwardSource = "auto" | "manual";

export type PortForwardRequest = {
	hostId: string;
	remotePath: string;
	threadId: string;
	remotePort: number;
	localPort: number;
	source: PortForwardSource;
};

export type PortDiscoveryRequest = {
	hostId: string;
	remotePath: string;
	threadId: string;
};

export type RemoteListeningPort = {
	remotePort: number;
	pid: number;
	command: string;
};

type ManagedTunnel = {
	request: PortForwardRequest;
	child: PortForwardChild;
};

type ManagedTunnelEntry = {
	id: string;
	tunnel: ManagedTunnel;
};

type PortForwardManagerOptions = {
	sshProgram?: string;
	startTimeoutMs?: number;
};

type PortForwardChild = ChildProcessByStdio<null, null, Readable>;
type CaptureChild = ChildProcessByStdio<null, Readable, Readable>;

const defaultStartTimeoutMs = 5000;
const sshCaptureTimeoutMs = 5000;
const childTerminateTimeoutMs = 500;

export function parsePort(value: JsonValue | undefined, field: string): number {
	if (typeof value !== "number" || !Number.isInteger(value)) {
		throw new Error(`${field} must be a number`);
	}
	if (value < 1 || value > 65535) {
		throw new Error(`${field} must be between 1 and 65535`);
	}
	return value;
}

export function requestFromPayload(
	payload: Record<string, JsonValue>,
): PortForwardRequest {
	const hostId = typeof payload.hostId === "string" ? payload.hostId.trim() : "";
	if (!hostId) throw new Error("Remote host id is required");
	const remotePath =
		typeof payload.remotePath === "string" ? payload.remotePath.trim() : "";
	if (!remotePath.startsWith("/")) {
		throw new Error("Remote path is required");
	}
	const threadId =
		typeof payload.threadId === "string" ? payload.threadId.trim() : "";
	if (!threadId) throw new Error("Thread id is required");
	const remotePort = parsePort(payload.remotePort, "remotePort");
	const localPort = parseLocalPort(payload.localPort);
	const rawSource = typeof payload.source === "string" ? payload.source : "manual";
	if (rawSource !== "auto" && rawSource !== "manual") {
		throw new Error("source must be auto or manual");
	}
	return { hostId, remotePath, threadId, remotePort, localPort, source: rawSource };
}

export function discoveryRequestFromPayload(
	payload: Record<string, JsonValue>,
): PortDiscoveryRequest {
	const hostId = typeof payload.hostId === "string" ? payload.hostId.trim() : "";
	if (!hostId) throw new Error("Remote host id is required");
	const remotePath =
		typeof payload.remotePath === "string" ? payload.remotePath.trim() : "";
	if (!remotePath.startsWith("/")) {
		throw new Error("Remote path is required");
	}
	const threadId =
		typeof payload.threadId === "string" ? payload.threadId.trim() : "";
	if (!threadId) throw new Error("Thread id is required");
	return { hostId, remotePath, threadId };
}

function parseLocalPort(value: JsonValue | undefined): number {
	if (value === 0) return 0;
	return parsePort(value, "localPort");
}

export function tunnelId(request: PortForwardRequest): string {
	return [
		request.hostId,
		request.remotePath,
		request.threadId,
		request.remotePort,
		request.localPort,
	].join(":");
}

export function localPortAvailable(port: number): Promise<boolean> {
	return new Promise((resolve) => {
		const server = createServer();
		server.once("error", () => resolve(false));
		server.listen(port, "127.0.0.1", () => {
			server.close(() => resolve(true));
		});
	});
}

export async function requestedLocalPortAvailable(port: number): Promise<boolean> {
	return port > 0 && (await localPortAvailable(port));
}

async function allocateFreeLocalPort(): Promise<number> {
	return new Promise((resolve, reject) => {
		const server = createServer();
		server.once("error", reject);
		server.listen(0, "127.0.0.1", () => {
			const address = server.address();
			if (!address || typeof address === "string") {
				server.close();
				reject(new Error("Cannot allocate a free local port"));
				return;
			}
			const port = address.port;
			server.close(() => resolve(port));
		});
	});
}

function sshTargetArg(target: SshTarget): string {
	const user = target.user.trim();
	return user ? `${user}@${target.host}` : target.host;
}

export function buildSshArgs(
	request: PortForwardRequest,
	target: SshTarget,
): string[] {
	const args = [
		"-N",
		"-o",
		"ExitOnForwardFailure=yes",
		...buildSshBaseOptions(),
		"-L",
		`127.0.0.1:${request.localPort}:127.0.0.1:${request.remotePort}`,
	];
	if (target.port !== null) args.push("-p", String(target.port));
	args.push("--", sshTargetArg(target));
	return args;
}

function buildSshBaseOptions(): string[] {
	return [
		"-o",
		"BatchMode=yes",
		"-o",
		"ControlMaster=no",
		"-o",
		"ControlPath=none",
		"-o",
		"ConnectTimeout=10",
		"-o",
		"ServerAliveInterval=15",
		"-o",
		"ServerAliveCountMax=4",
	];
}

function buildSshBaseArgs(target: SshTarget): string[] {
	const args = buildSshBaseOptions();
	if (target.port !== null) args.push("-p", String(target.port));
	args.push("--", sshTargetArg(target));
	return args;
}

function remoteDiscoveryScript(): string {
	return `
set -eu
if ! command -v lsof >/dev/null 2>&1; then
  echo "Remote lsof is required for port discovery" >&2
  exit 127
fi
for pid in $(lsof -nP -iTCP -sTCP:LISTEN -t 2>/dev/null | sort -u); do
  command=$(ps -p "$pid" -o comm= 2>/dev/null | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')
  cwd=$(lsof -a -p "$pid" -d cwd -Fn 2>/dev/null | sed -n 's/^n//p' | head -n 1)
  lsof -nP -a -p "$pid" -iTCP -sTCP:LISTEN -Fn 2>/dev/null | sed -n 's/^n//p' | while IFS= read -r address; do
    printf '%s\t%s\t%s\t%s\n' "$pid" "$command" "$cwd" "$address"
  done
done
`;
}

function parsePortFromAddress(address: string): number | null {
	const match = address.trim().match(/(?:^|:)([0-9]{1,5})(?:\s|$)/);
	if (!match) return null;
	const port = Number(match[1]);
	if (!Number.isInteger(port) || port < 1 || port > 65535) return null;
	return port;
}

function pathInsideWorkspace(path: string, workspace: string): boolean {
	const normalizedPath = path.replace(/\/+$/, "");
	const normalizedWorkspace = workspace.replace(/\/+$/, "");
	if (!normalizedWorkspace || normalizedWorkspace === "/") return false;
	return (
		normalizedPath === normalizedWorkspace ||
		normalizedPath.startsWith(`${normalizedWorkspace}/`)
	);
}

export function parseRemoteListeningPorts(
	output: string,
	remotePath: string,
): RemoteListeningPort[] {
	const ports = new Map<number, RemoteListeningPort>();
	for (const line of output.split("\n")) {
		if (!line.trim()) continue;
		const [rawPid, command = "", cwd = "", address = ""] = line.split("\t");
		if (!pathInsideWorkspace(cwd, remotePath)) continue;
		const pid = Number(rawPid);
		if (!Number.isInteger(pid) || pid < 1) continue;
		const remotePort = parsePortFromAddress(address);
		if (!remotePort || remotePort < 1024) continue;
		if (!ports.has(remotePort)) {
			ports.set(remotePort, { remotePort, pid, command });
		}
	}
	return Array.from(ports.values()).sort(
		(a, b) => a.remotePort - b.remotePort,
	);
}

function childStillRunning(child: PortForwardChild): boolean {
	return child.exitCode === null && child.signalCode === null;
}

function tunnelStillRunning(tunnel: ManagedTunnel): boolean {
	return childStillRunning(tunnel.child);
}

function killChild(child: PortForwardChild): void {
	if (!childStillRunning(child)) return;
	child.kill("SIGTERM");
	const timer = setTimeout(() => {
		if (childStillRunning(child)) child.kill("SIGKILL");
	}, childTerminateTimeoutMs);
	timer.unref?.();
}

function waitForChildExit(
	child: PortForwardChild,
	timeoutMs = childTerminateTimeoutMs,
): Promise<void> {
	if (!childStillRunning(child)) return Promise.resolve();
	return new Promise((resolve) => {
		let settled = false;
		const finish = () => {
			if (settled) return;
			settled = true;
			clearTimeout(timer);
			child.removeListener("exit", finish);
			child.removeListener("close", finish);
			resolve();
		};
		const timer = setTimeout(finish, timeoutMs);
		child.once("exit", finish);
		child.once("close", finish);
	});
}

async function terminateChild(child: PortForwardChild): Promise<void> {
	if (!childStillRunning(child)) return;
	child.kill("SIGTERM");
	await waitForChildExit(child);
	if (!childStillRunning(child)) return;
	child.kill("SIGKILL");
	await waitForChildExit(child);
}

function sameRemotePortRequest(
	left: PortForwardRequest,
	right: PortForwardRequest,
): boolean {
	return (
		left.hostId === right.hostId &&
		left.remotePath === right.remotePath &&
		left.threadId === right.threadId &&
		left.remotePort === right.remotePort
	);
}

function tunnelResponse(id: string, tunnel: ManagedTunnel): Record<string, JsonValue> {
	return {
		status: "ok",
		id,
		localPort: tunnel.request.localPort,
		localUrl: `http://127.0.0.1:${tunnel.request.localPort}`,
	};
}

function delay(ms: number): Promise<void> {
	return new Promise((resolve) => setTimeout(resolve, ms));
}

function captureProcess(
	program: string,
	args: string[],
	timeoutMs = sshCaptureTimeoutMs,
): Promise<string> {
	return new Promise((resolve, reject) => {
		let child: CaptureChild;
		try {
			child = spawn(program, args, { stdio: ["ignore", "pipe", "pipe"] });
		} catch (error) {
			reject(error);
			return;
		}
		const stdout: string[] = [];
		const stderr: string[] = [];
		let settled = false;
		const finish = (error?: Error) => {
			if (settled) return;
			settled = true;
			clearTimeout(timer);
			child.removeAllListeners("error");
			if (error) reject(error);
			else resolve(stdout.join(""));
		};
		const timer = setTimeout(() => {
			if (child.exitCode === null && child.signalCode === null) {
				child.kill("SIGTERM");
				setTimeout(() => {
					if (child.exitCode === null && child.signalCode === null) {
						child.kill("SIGKILL");
					}
				}, 100);
			}
			finish(new Error("Remote port discovery timed out"));
		}, timeoutMs);
		child.stdout.setEncoding("utf8");
		child.stderr.setEncoding("utf8");
		child.stdout.on("data", (chunk) => stdout.push(String(chunk)));
		child.stderr.on("data", (chunk) => stderr.push(String(chunk)));
		child.once("error", (error) => finish(error));
		child.once("close", (code) => {
			if (code === 0) {
				finish();
				return;
			}
			const detail = stderr.join("").trim();
			finish(
				new Error(
					detail || `Remote port discovery exited with status ${String(code)}`,
				),
			);
		});
	});
}

export async function discoverRemoteListeningPorts(
	request: PortDiscoveryRequest,
	target: SshTarget,
): Promise<RemoteListeningPort[]> {
	const output = await captureProcess("ssh", [
		...buildSshBaseArgs(target),
		"sh",
		"-lc",
		remoteDiscoveryScript(),
	]);
	return parseRemoteListeningPorts(output, request.remotePath);
}

export class PortForwardManager {
	private readonly tunnels = new Map<string, ManagedTunnel>();
	private readonly sshProgram: string;
	private readonly startTimeoutMs: number;

	constructor(options: PortForwardManagerOptions = {}) {
		this.sshProgram = options.sshProgram ?? "ssh";
		this.startTimeoutMs = options.startTimeoutMs ?? defaultStartTimeoutMs;
	}

	private pruneExitedTunnels(): void {
		for (const [id, tunnel] of this.tunnels) {
			if (!tunnelStillRunning(tunnel)) this.tunnels.delete(id);
		}
	}

	private reusableTunnel(request: PortForwardRequest): ManagedTunnelEntry | null {
		const exactId = request.localPort > 0 ? tunnelId(request) : "";
		for (const [id, tunnel] of this.tunnels) {
			if (!tunnelStillRunning(tunnel)) continue;
			if (exactId && id === exactId) return { id, tunnel };
			if (request.source === "auto" && sameRemotePortRequest(request, tunnel.request)) {
				return { id, tunnel };
			}
		}
		return null;
	}

	async list(): Promise<Record<string, JsonValue>> {
		this.pruneExitedTunnels();
		const ports = Array.from(this.tunnels.entries()).map(([id, tunnel]) => ({
			id,
			status: "active",
			hostId: tunnel.request.hostId,
			remotePath: tunnel.request.remotePath,
			threadId: tunnel.request.threadId,
			remotePort: tunnel.request.remotePort,
			localPort: tunnel.request.localPort,
			localUrl: `http://127.0.0.1:${tunnel.request.localPort}`,
			source: tunnel.request.source,
		}));
		return { status: "ok", ports };
	}

	async start(
		request: PortForwardRequest,
		target: SshTarget,
	): Promise<Record<string, JsonValue>> {
		this.pruneExitedTunnels();
		const reusableBeforeAllocation = this.reusableTunnel(request);
		if (reusableBeforeAllocation) {
			return tunnelResponse(reusableBeforeAllocation.id, reusableBeforeAllocation.tunnel);
		}

		const localPort = (await requestedLocalPortAvailable(request.localPort))
			? request.localPort
			: await allocateFreeLocalPort();
		const startRequest = { ...request, localPort };
		const reusableAfterAllocation = this.reusableTunnel(startRequest);
		if (reusableAfterAllocation) {
			return tunnelResponse(reusableAfterAllocation.id, reusableAfterAllocation.tunnel);
		}

		const args = buildSshArgs(startRequest, target);

		let child: PortForwardChild;
		try {
			child = spawn(this.sshProgram, args, {
				stdio: ["ignore", "ignore", "pipe"],
			});
		} catch (error) {
			return {
				status: "failed",
				message: error instanceof Error ? error.message : String(error),
			};
		}
		const startErrorPromise = this.waitForTunnelStart(
			child,
			startRequest.localPort,
		);
		const stderr: string[] = [];
		child.stderr.setEncoding("utf8");
		child.stderr.on("data", (chunk) => {
			stderr.push(String(chunk));
		});

		const startError = await startErrorPromise;
		if (startError) {
			await terminateChild(child);
			const detail = stderr.join("").trim();
			return {
				status: "failed",
				message: detail ? `${startError}: ${detail}` : startError,
			};
		}

		const id = tunnelId(startRequest);
		const localUrl = `http://127.0.0.1:${startRequest.localPort}`;
		this.tunnels.set(id, { request: startRequest, child });
		return { status: "ok", id, localPort: startRequest.localPort, localUrl };
	}

	async stop(id: string): Promise<Record<string, JsonValue>> {
		const tunnel = this.tunnels.get(id);
		if (!tunnel) {
			return { status: "failed", message: "Port tunnel not found" };
		}
		this.tunnels.delete(id);
		await terminateChild(tunnel.child);
		return { status: "ok", id };
	}

	stopAll(): void {
		for (const tunnel of this.tunnels.values()) {
			killChild(tunnel.child);
		}
		this.tunnels.clear();
	}

	private async waitForTunnelStart(
		child: PortForwardChild,
		localPort: number,
	): Promise<string> {
		return new Promise((resolve) => {
			let settled = false;
			const finish = (message: string) => {
				if (settled) return;
				settled = true;
				child.removeListener("error", onError);
				resolve(message);
			};
			const onError = (error: Error) => {
				finish(error.message);
			};
			child.once("error", onError);

			const poll = async () => {
				const deadline = Date.now() + this.startTimeoutMs;
				while (Date.now() < deadline) {
					if (!childStillRunning(child)) {
						finish(
							`SSH tunnel exited before forwarding started: ${child.exitCode ?? child.signalCode}`,
						);
						return;
					}
					if (!(await localPortAvailable(localPort))) {
						finish("");
						return;
					}
					await delay(50);
				}
				finish("SSH tunnel did not start listening on the local port");
			};

			poll().catch((error: unknown) => {
				finish(error instanceof Error ? error.message : String(error));
			});
		});
	}
	}
