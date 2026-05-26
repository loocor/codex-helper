import { spawn } from "node:child_process";
import {
	appendFileSync,
	mkdirSync,
	readdirSync,
	readFileSync,
	writeFileSync,
} from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { listTargets, pickCodexPageTarget } from "./cdp";
import {
	PortForwardManager,
	discoverRemoteListeningPorts,
	discoveryRequestFromPayload,
	requestFromPayload,
} from "./ports";
import { invokeRustBridge, isRustBridgePath } from "./rust-bridge";

import {
	fallbackOpenRequestResponse,
	openZedRemote,
	resolveSshTargetForHostId,
	resolveSshTargetResponse,
	zedRemoteStatus,
} from "./zed";

type JsonValue =
	| null
	| boolean
	| number
	| string
	| JsonValue[]
	| { [key: string]: JsonValue };

type HelperSettings = {
	markdownExportEnabled: boolean;
	sessionMoveEnabled: boolean;
	portForwardingEnabled: boolean;
	portAutoForwardWeb: boolean;
	portSameLocalPort: boolean;
};

const defaultSettings: HelperSettings = {
	markdownExportEnabled: false,
	sessionMoveEnabled: false,
	portForwardingEnabled: false,
	portAutoForwardWeb: true,
	portSameLocalPort: true,
};

const portManager = new PortForwardManager();

export function stopPortForwards(): void {
	portManager.stopAll();
}

function helperRoot(): string {
	const configured = process.env.CODEX_HELPER_HOME?.trim();
	return configured || join(homedir(), ".codex-helper");
}

function helperLogsDir(): string {
	return join(helperRoot(), "logs");
}

function helperScriptsDir(): string {
	return join(helperRoot(), "scripts");
}

function helperConfigPath(): string {
	return join(helperRoot(), "config.json");
}

function ensureHelperRoot(): void {
	mkdirSync(helperRoot(), { recursive: true });
	mkdirSync(helperLogsDir(), { recursive: true });
	mkdirSync(helperScriptsDir(), { recursive: true });
}

function logPath(): string {
	ensureHelperRoot();
	const logsDir = helperLogsDir();
	return join(logsDir, "codex-helper.jsonl");
}

function appendDiagnostic(event: string, detail: JsonValue): void {
	const record = {
		timestamp: new Date().toISOString(),
		event,
		detail,
	};
	appendFileSync(logPath(), `${JSON.stringify(record)}\n`, "utf8");
}

function readSettings(): HelperSettings {
	ensureHelperRoot();
	try {
		const settings = JSON.parse(
			readFileSync(helperConfigPath(), "utf8"),
		) as Partial<HelperSettings>;
		const next: HelperSettings = { ...defaultSettings };
		for (const key of Object.keys(defaultSettings) as Array<keyof HelperSettings>) {
			if (typeof settings[key] === "boolean") next[key] = settings[key];
		}
		return next;
	} catch {
		writeFileSync(
			helperConfigPath(),
			`${JSON.stringify(defaultSettings, null, 2)}\n`,
			"utf8",
		);
		return { ...defaultSettings };
	}
}

function updateSettings(payload: Record<string, JsonValue>): HelperSettings {
	const current = readSettings();
	const next: HelperSettings = { ...current };
	for (const [key, value] of Object.entries(payload)) {
		if (!(key in defaultSettings)) {
			throw new Error(`Unknown settings key: ${key}`);
		}
		if (typeof value !== "boolean") {
			throw new Error(`Settings value for ${key} must be a boolean`);
		}
		(next as Record<string, boolean>)[key] = value;
	}
	writeFileSync(
		helperConfigPath(),
		`${JSON.stringify(next, null, 2)}\n`,
		"utf8",
	);
	return next;
}

function listUserScripts(): string[] {
	ensureHelperRoot();
	const files = readdirSync(helperScriptsDir());
	return files.filter((name) => name.endsWith(".js")).sort();
}

function readLatestLogContents(): string {
	try {
		const contents = readFileSync(logPath(), "utf8");
		const lines = contents.split("\n").filter(Boolean);
		return lines.slice(-80).join("\n");
	} catch {
		return "";
	}
}

function openPath(path: string, reveal = false): JsonValue {
	try {
		const args = reveal ? ["-R", path] : [path];
		const child = spawn("open", args, { stdio: "ignore", detached: true });
		child.unref();
		return { status: "ok", path };
	} catch (error) {
		return {
			status: "failed",
			message: error instanceof Error ? error.message : String(error),
		};
	}
}

function localBrowserUrlFromPayload(payload: Record<string, JsonValue>): string {
	const raw = typeof payload.url === "string" ? payload.url.trim() : "";
	if (!raw) throw new Error("URL is required");
	let url: URL;
	try {
		url = new URL(raw);
	} catch {
		throw new Error("URL is invalid");
	}
	const host = url.hostname.toLowerCase().replace(/^\[|\]$/g, "");
	if (url.protocol !== "http:" && url.protocol !== "https:") {
		throw new Error("Only http(s) URLs can be opened");
	}
	if (!["localhost", "127.0.0.1", "::1", "0.0.0.0"].includes(host)) {
		throw new Error("Only local forwarded URLs can be opened");
	}
	if (!url.port) throw new Error("Local forwarded URL must include a port");
	return url.toString();
}

function devtoolsUrl(
	debugPort: number,
	target: { webSocketDebuggerUrl?: string },
): string {
	const ws = (target.webSocketDebuggerUrl || "").trim();
	if (!ws.startsWith("ws://")) {
		throw new Error("Selected Codex DevTools target has no websocket URL");
	}
	return `http://127.0.0.1:${debugPort}/devtools/inspector.html?ws=${ws.slice(5)}`;
}

function helperDebugPort(): number {
	const raw = process.env.CODEX_HELPER_DEBUG_PORT || "9229";
	const value = Number(raw);
	if (!Number.isInteger(value) || value < 1 || value > 65535) return 9229;
	return value;
}

export async function handleBridgeRequest(
	path: string,
	payload: Record<string, JsonValue>,
): Promise<JsonValue> {
	if (isRustBridgePath(path)) {
		return invokeRustBridge(path, payload);
	}
	switch (path) {
		case "/backend/status":
			return { status: "ok", message: "Codex Helper backend connected" };
		case "/diagnostics/log": {
			const event =
				typeof payload.event === "string" ? payload.event : "renderer.event";
			appendDiagnostic(event, payload);
			return { status: "ok" };
		}
		case "/runtime/user-scripts":
			return {
				status: "ok",
				path: helperScriptsDir(),
				scripts: listUserScripts(),
			};
		case "/settings/get":
			return { status: "ok", settings: readSettings() };
		case "/settings/set":
			try {
				return { status: "ok", settings: updateSettings(payload) };
			} catch (error) {
				return {
					status: "failed",
					message: error instanceof Error ? error.message : String(error),
				};
			}
		case "/diagnostics/read-latest":
			return {
				status: "ok",
				path: logPath(),
				contents: readLatestLogContents(),
			};
		case "/diagnostics/reveal-log":
			return openPath(logPath(), true);
		case "/logs/reveal":
			return openPath(helperLogsDir());
		case "/scripts/reveal":
			return openPath(helperScriptsDir());
		case "/state/reveal":
			return openPath(helperRoot());
		case "/devtools/open":
			try {
				const debugPort = helperDebugPort();
				const targets = await listTargets(debugPort);
				const target = pickCodexPageTarget(targets);
				const url = devtoolsUrl(debugPort, target);
				return openPath(url);
			} catch (error) {
				return {
					status: "failed",
					message: error instanceof Error ? error.message : String(error),
				};
			}
		case "/url/open-external":
			try {
				return openPath(localBrowserUrlFromPayload(payload));
			} catch (error) {
				return {
					status: "failed",
					message: error instanceof Error ? error.message : String(error),
				};
			}
		case "/ports/list":
			return portManager.list();
		case "/ports/discover":
			try {
				const request = discoveryRequestFromPayload(payload);
				const target = resolveSshTargetForHostId(request.hostId);
				const ports = await discoverRemoteListeningPorts(request, target);
				return {
					status: "ok",
					hostId: request.hostId,
					remotePath: request.remotePath,
					threadId: request.threadId,
					ports,
				};
			} catch (error) {
				return {
					status: "failed",
					message: error instanceof Error ? error.message : String(error),
				};
			}
		case "/ports/forward":
			try {
				const request = requestFromPayload(payload);
				const target = resolveSshTargetForHostId(request.hostId);
				return portManager.start(request, target);
			} catch (error) {
				return {
					status: "failed",
					message: error instanceof Error ? error.message : String(error),
				};
			}
		case "/ports/stop": {
			const id = typeof payload.id === "string" ? payload.id : "";
			return portManager.stop(id);
		}
		case "/zed-remote/status":
			return zedRemoteStatus();
		case "/zed-remote/resolve-host":
			return resolveSshTargetResponse(payload);
		case "/zed-remote/fallback-request":
			return fallbackOpenRequestResponse();
		case "/zed-remote/open":
			return openZedRemote(payload);
		default:
			return {
				status: "failed",
				message: `Unknown Codex Helper bridge path: ${path}`,
			};
	}
}
