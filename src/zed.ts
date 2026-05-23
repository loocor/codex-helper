import { spawn } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

type JsonValue =
	| null
	| boolean
	| number
	| string
	| JsonValue[]
	| { [key: string]: JsonValue };

type SshTarget = {
	user: string;
	host: string;
	port: number | null;
};

class ZedRemoteError extends Error {
	constructor(message: string) {
		super(message);
		this.name = "ZedRemoteError";
	}
}

function stringValue(value: unknown): string {
	if (typeof value === "string") return value.trim();
	if (typeof value === "number") return String(value);
	return "";
}

function orElseNonEmpty(primary: string, fallback: () => string): string {
	return primary || fallback();
}

function candidateZedAppPaths(): string[] {
	return [
		"/Applications/Zed.app",
		"/Applications/Zed Preview.app",
		"/Applications/Zed Nightly.app",
		join(homedir(), "Applications/Zed.app"),
		join(homedir(), "Applications/Zed Preview.app"),
		join(homedir(), "Applications/Zed Nightly.app"),
	];
}

function findZedAppPath(): string | null {
	return candidateZedAppPaths().find((path) => existsSync(path)) ?? null;
}

function findZedCliPath(): string {
	const pathVar = process.env.PATH ?? "";
	for (const dir of pathVar.split(":")) {
		const candidate = join(dir, "zed");
		if (existsSync(candidate)) return candidate;
	}
	return "";
}

export function zedRemoteStatus(): JsonValue {
	const appPath = findZedAppPath();
	const cliPath = findZedCliPath();
	const platformSupported =
		process.platform === "darwin" ||
		process.platform === "win32" ||
		process.platform === "linux";
	return {
		status: platformSupported ? "ok" : "failed",
		platformSupported,
		zedAppFound: appPath !== null,
		zedCliFound: cliPath.length > 0,
		zedAppPath: appPath ?? "",
		zedCliPath: cliPath,
	};
}

function parsePortStr(value: string): number | null {
	if (!value) return null;
	if (!/^\d+$/.test(value)) throw new ZedRemoteError("Invalid SSH port");
	const port = Number(value);
	if (!Number.isInteger(port) || port < 1 || port > 65535) {
		throw new ZedRemoteError("Invalid SSH port");
	}
	return port;
}

function parsePortValue(value: unknown): number | null {
	if (value === null || value === undefined) return null;
	if (typeof value === "string" && value.trim() === "") return null;
	if (typeof value === "string") return parsePortStr(value.trim());
	if (typeof value === "number") {
		if (!Number.isInteger(value) || value < 1 || value > 65535) {
			throw new ZedRemoteError("Invalid SSH port");
		}
		return value;
	}
	throw new ZedRemoteError("Invalid SSH port");
}

export function splitSshAuthority(value: string): {
	user: string;
	host: string;
	port: number | null;
} {
	let authority = value.trim();
	if (!authority) return { user: "", host: "", port: null };

	let user = "";
	const atIndex = authority.lastIndexOf("@");
	if (atIndex >= 0) {
		user = authority.slice(0, atIndex).trim();
		authority = authority.slice(atIndex + 1);
	}

	if (authority.startsWith("[")) {
		const closeIndex = authority.indexOf("]");
		if (closeIndex >= 0) {
			const host = authority.slice(0, closeIndex + 1).trim();
			const suffix = authority.slice(closeIndex + 1);
			const port = suffix.startsWith(":")
				? parsePortStr(suffix.slice(1))
				: null;
			return { user, host, port };
		}
		return { user, host: authority.trim(), port: null };
	}

	if (authority.split(":").length === 2) {
		const parts = authority.split(":");
		const host = parts[0] ?? "";
		const rawPort = parts[1] ?? "";
		if (rawPort && /^\d+$/.test(rawPort)) {
			return { user, host: host.trim(), port: parsePortStr(rawPort) };
		}
	}

	return { user, host: authority.trim(), port: null };
}

function validateSshHost(host: string): string {
	const trimmed = host.trim();
	if (!trimmed) {
		throw new ZedRemoteError("Cannot determine remote SSH host for this file");
	}
	if (
		/[\s/?#@]/.test(trimmed) ||
		[...trimmed].some((ch) => ch.charCodeAt(0) <= 0x1f)
	) {
		throw new ZedRemoteError("Invalid SSH host");
	}
	if (trimmed.startsWith("[") || trimmed.endsWith("]")) {
		if (!(trimmed.startsWith("[") && trimmed.endsWith("]"))) {
			throw new ZedRemoteError("Invalid SSH host");
		}
	}
	if (trimmed.includes("[") && !trimmed.startsWith("[")) {
		throw new ZedRemoteError("Invalid SSH host");
	}
	return trimmed;
}

function percentEncodeSegment(segment: string): string {
	let encoded = "";
	for (const byte of Buffer.from(segment, "utf8")) {
		const ch = String.fromCharCode(byte);
		if (/[A-Za-z0-9\-._~]/.test(ch)) encoded += ch;
		else encoded += `%${byte.toString(16).toUpperCase().padStart(2, "0")}`;
	}
	return encoded;
}

function encodeRemotePath(path: string): string {
	if (!path) throw new ZedRemoteError("Remote path is required");
	if (!path.startsWith("/")) {
		throw new ZedRemoteError("Remote path must be absolute");
	}
	return path.split("/").map(percentEncodeSegment).join("/");
}

export function buildZedRemoteUrl(target: SshTarget, path: string): string {
	const host = validateSshHost(target.host);
	const userPrefix = target.user.trim()
		? `${percentEncodeSegment(target.user.trim())}@`
		: "";
	const portSuffix = target.port && target.port > 0 ? `:${target.port}` : "";
	const encodedPath = encodeRemotePath(path);
	return `ssh://${userPrefix}${host}${portSuffix}${encodedPath}`;
}

function targetFromPayload(payload: Record<string, JsonValue>): SshTarget {
	const ssh = (payload.ssh ?? {}) as Record<string, JsonValue>;
	const rawHost = orElseNonEmpty(stringValue(ssh.host), () =>
		orElseNonEmpty(stringValue(ssh.hostname), () => stringValue(ssh.hostName)),
	);
	const {
		user: authorityUser,
		host: authorityHost,
		port: authorityPort,
	} = splitSshAuthority(rawHost);
	const user = orElseNonEmpty(stringValue(ssh.user), () =>
		orElseNonEmpty(stringValue(ssh.username), () => authorityUser),
	);
	const host = validateSshHost(authorityHost);
	let port = authorityPort;
	if ("port" in ssh) {
		if (ssh.port === null) port = authorityPort;
		else if (typeof ssh.port === "string" && ssh.port.trim() === "") {
			port = authorityPort;
		} else {
			port = parsePortValue(ssh.port);
		}
	}
	return { user, host, port };
}

export function codexGlobalStatePath(): string {
	const codexHome = process.env.CODEX_HOME?.trim();
	if (codexHome) return join(codexHome, ".codex-global-state.json");
	return join(homedir(), ".codex", ".codex-global-state.json");
}

function readGlobalState(): Record<string, JsonValue> {
	const path = codexGlobalStatePath();
	const data = readFileSync(path, "utf8");
	return JSON.parse(data) as Record<string, JsonValue>;
}

function targetFromManagedRemoteConnection(
	connection: Record<string, JsonValue>,
): SshTarget {
	const sshHost = orElseNonEmpty(stringValue(connection.sshHost), () =>
		stringValue(connection.hostname),
	);
	const {
		user: authorityUser,
		host: authorityHost,
		port: authorityPort,
	} = splitSshAuthority(sshHost);
	const host = orElseNonEmpty(authorityHost, () =>
		orElseNonEmpty(stringValue(connection.sshAlias), () =>
			stringValue(connection.alias),
		),
	);
	const user = orElseNonEmpty(stringValue(connection.sshUser), () =>
		orElseNonEmpty(stringValue(connection.user), () => authorityUser),
	);
	let port = authorityPort;
	if ("sshPort" in connection) {
		if (connection.sshPort === null) port = authorityPort;
		else if (
			typeof connection.sshPort === "string" &&
			connection.sshPort.trim() === ""
		) {
			port = authorityPort;
		} else {
			port = parsePortValue(connection.sshPort);
		}
	}
	return { user, host: validateSshHost(host), port };
}

function resolveSshTargetFromGlobalState(
	state: Record<string, JsonValue>,
	hostId: string,
): SshTarget {
	if (!hostId) throw new ZedRemoteError("Remote host id is required");
	const connections = Array.isArray(state["codex-managed-remote-connections"])
		? (state["codex-managed-remote-connections"] as JsonValue[])
		: [];
	for (const connection of connections) {
		if (!connection || typeof connection !== "object") continue;
		const record = connection as Record<string, JsonValue>;
		if (stringValue(record.hostId) !== hostId) continue;
		return targetFromManagedRemoteConnection(record);
	}
	throw new ZedRemoteError("Cannot resolve remote SSH host for this file");
}

function orderedRemoteProjectsFromGlobalState(
	state: Record<string, JsonValue>,
): Record<string, JsonValue>[] {
	const projects = Array.isArray(state["remote-projects"])
		? (state["remote-projects"] as JsonValue[])
				.filter((project) => project && typeof project === "object")
				.map((project) => project as Record<string, JsonValue>)
		: [];
	const projectOrder = Array.isArray(state["project-order"])
		? (state["project-order"] as JsonValue[]).map((item) => stringValue(item))
		: [];
	const ordered: Record<string, JsonValue>[] = [];
	const orderedIds = new Set<string>();
	for (const projectId of projectOrder) {
		const project = projects.find(
			(entry) => stringValue(entry.id) === projectId,
		);
		if (project) {
			ordered.push(project);
			orderedIds.add(projectId);
		}
	}
	for (const project of projects) {
		const id = stringValue(project.id);
		if (!orderedIds.has(id)) ordered.push(project);
	}
	return ordered;
}

function fallbackOpenRequestFromGlobalState(
	state: Record<string, JsonValue>,
): Record<string, JsonValue> {
	const selectedHostId = stringValue(state["selected-remote-host-id"]);
	const selectedProject = orderedRemoteProjectsFromGlobalState(state).find(
		(project) => {
			const projectHostId = stringValue(project.hostId);
			const remotePath = stringValue(project.remotePath);
			return (
				(selectedHostId === "" || projectHostId === selectedHostId) &&
				remotePath.startsWith("/")
			);
		},
	);
	if (!selectedProject) {
		throw new ZedRemoteError(
			"Cannot determine remote workspace or file for Zed",
		);
	}
	const hostId = orElseNonEmpty(selectedHostId, () =>
		stringValue(selectedProject.hostId),
	);
	if (!hostId) throw new ZedRemoteError("Remote host id is required");
	const target = resolveSshTargetFromGlobalState(state, hostId);
	return {
		hostId,
		ssh: { user: target.user, host: target.host, port: target.port },
		path: stringValue(selectedProject.remotePath),
	};
}

function launchZedUrl(url: string): void {
	const appPath = findZedAppPath();
	const cliPath = findZedCliPath();
	if (process.platform === "darwin" && appPath) {
		const child = spawn("open", ["-a", appPath, url], {
			detached: true,
			stdio: "ignore",
		});
		child.unref();
		return;
	}
	if (cliPath) {
		const child = spawn(cliPath, [url], { detached: true, stdio: "ignore" });
		child.unref();
		return;
	}
	throw new ZedRemoteError("Zed is not installed or not available on PATH");
}

export function resolveSshTargetResponse(
	payload: Record<string, JsonValue>,
): JsonValue {
	try {
		const hostId = stringValue(payload.hostId);
		const target = resolveSshTargetFromGlobalState(readGlobalState(), hostId);
		return {
			status: "ok",
			ssh: { user: target.user, host: target.host, port: target.port },
		};
	} catch (error) {
		return {
			status: "failed",
			message: error instanceof Error ? error.message : String(error),
		};
	}
}

export function fallbackOpenRequestResponse(): JsonValue {
	try {
		const request = fallbackOpenRequestFromGlobalState(readGlobalState());
		return { status: "ok", request };
	} catch (error) {
		return {
			status: "failed",
			message: error instanceof Error ? error.message : String(error),
		};
	}
}

export function openZedRemote(payload: Record<string, JsonValue>): JsonValue {
	try {
		const target = targetFromPayload(payload);
		const path = stringValue(payload.path);
		const url = buildZedRemoteUrl(target, path);
		launchZedUrl(url);
		return { status: "ok", url };
	} catch (error) {
		return {
			status: "failed",
			message: error instanceof Error ? error.message : String(error),
		};
	}
}
