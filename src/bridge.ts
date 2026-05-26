import { readFileSync } from "node:fs";

import { browserWebsocketUrl } from "./cdp";
import type { LaunchTimer } from "./debug";
import { handleBridgeRequest } from "./routes";

const BRIDGE_BINDING_NAME = "codexHelperBridgeV1";
const CDP_COMMAND_TIMEOUT_MS = 5000;
const DEFAULT_BRIDGE_REQUEST_TIMEOUT_MS = 10000;
const LONG_BRIDGE_REQUEST_TIMEOUT_MS = 120000;

type JsonValue =
	| null
	| boolean
	| number
	| string
	| JsonValue[]
	| { [key: string]: JsonValue };

type CdpMessage = {
	id?: number;
	method?: string;
	params?: Record<string, JsonValue>;
	result?: JsonValue;
	error?: JsonValue;
	sessionId?: string;
};

let nextMessageId = 100;

function buildBridgeScript(bindingName: string): string {
	return `
(() => {
  window.__codexHelperCallbacks = new Map();
  window.__codexHelperSeq = 0;
  window.__codexHelperResolve = (id, result) => {
    const callback = window.__codexHelperCallbacks.get(id);
    if (!callback) return;
    window.__codexHelperCallbacks.delete(id);
    callback.resolve(result);
  };
  window.__codexHelperReject = (id, message) => {
    const callback = window.__codexHelperCallbacks.get(id);
    if (!callback) return;
    window.__codexHelperCallbacks.delete(id);
    callback.resolve({ status: "failed", message });
  };
  window.__codexHelperBridge = (path, payload = {}) => new Promise((resolve) => {
    const id = String(++window.__codexHelperSeq);
    window.__codexHelperCallbacks.set(id, { resolve });
    window.${bindingName}(JSON.stringify({ id, path, payload }));
  });
})();
`;
}

function runtimeEvaluateParams(expression: string): JsonValue {
	return {
		expression,
		awaitPromise: false,
		allowUnsafeEvalBlockedByCSP: true,
	};
}

function cdpCommand(
	id: number,
	method: string,
	params: JsonValue,
	sessionId?: string,
): string {
	const command: CdpMessage = {
		id,
		method,
		params: params as Record<string, JsonValue>,
	};
	if (sessionId) command.sessionId = sessionId;
	return JSON.stringify(command);
}

async function withBridgeRequestTimeout<T>(
	promise: Promise<T>,
	path: string,
): Promise<T> {
	let timer: ReturnType<typeof setTimeout> | undefined;
	const timeoutMs = bridgeRequestTimeoutMs(path);
	const timeoutPromise = new Promise<T>((_, reject) => {
		timer = setTimeout(() => {
			reject(new Error(bridgeRequestTimeoutMessage(path, timeoutMs)));
		}, timeoutMs);
	});
	try {
		return await Promise.race([promise, timeoutPromise]);
	} finally {
		if (timer) clearTimeout(timer);
	}
}

export function bridgeRequestTimeoutMs(path: string): number {
	switch (path) {
		case "/auto-rename-chat":
		case "/export-markdown":
			return LONG_BRIDGE_REQUEST_TIMEOUT_MS;
		default:
			return DEFAULT_BRIDGE_REQUEST_TIMEOUT_MS;
	}
}

export function bridgeRequestTimeoutMessage(
	path: string,
	timeoutMs = bridgeRequestTimeoutMs(path),
): string {
	const seconds = Math.round(timeoutMs / 1000);
	switch (path) {
		case "/auto-rename-chat":
			return `Regenerate chat title is still running after ${seconds}s. The chat may be large, the model request may be slow, or the remote host may be unreachable. Please retry when the connection is stable.`;
		case "/export-markdown":
			return `Markdown export is still running after ${seconds}s. The chat may be large, the model request may be slow, or the remote host may be unreachable. Please retry when the connection is stable.`;
		default:
			return `Bridge request ${path || "(unknown)"} timed out after ${timeoutMs}ms`;
	}
}

class BindingCdpSession {
	private socket: WebSocket;
	private responses = new Map<number, CdpMessage>();
	private bindingCalls: CdpMessage[] = [];
	private sessionId?: string;
	private closed = false;

	constructor(socket: WebSocket) {
		this.socket = socket;
		this.socket.addEventListener("message", (event) => {
			const message = JSON.parse(String(event.data)) as CdpMessage;
			if (message.method === "Runtime.bindingCalled") {
				this.bindingCalls.push(message);
				return;
			}
			if (message.id !== undefined) {
				this.responses.set(message.id, message);
			}
		});
		this.socket.addEventListener("close", () => {
			this.closed = true;
		});
	}

	withSessionId(sessionId: string): this {
		this.sessionId = sessionId;
		return this;
	}

	async sendCommand(
		id: number,
		method: string,
		params: JsonValue,
	): Promise<CdpMessage> {
		this.socket.send(cdpCommand(id, method, params, this.sessionId));
		return this.waitForResponse(id, method);
	}

	async sendCommandWithoutWait(
		id: number,
		method: string,
		params: JsonValue,
	): Promise<void> {
		this.socket.send(cdpCommand(id, method, params, this.sessionId));
	}

	private async waitForResponse(
		id: number,
		method: string,
	): Promise<CdpMessage> {
		const startedAt = Date.now();
		while (!this.closed) {
			const response = this.responses.get(id);
			if (response) {
				this.responses.delete(id);
				if (response.error) {
					throw new Error(
						`CDP command ${method} failed: ${JSON.stringify(response.error)}`,
					);
				}
				return response;
			}
			if (Date.now() - startedAt > CDP_COMMAND_TIMEOUT_MS) {
				throw new Error(
					`Timed out waiting for CDP command ${method} after ${CDP_COMMAND_TIMEOUT_MS}ms`,
				);
			}
			await Bun.sleep(10);
		}
		throw new Error(`CDP command ${method} closed before response`);
	}

	async drainBindingQueue(): Promise<void> {
		while (this.bindingCalls.length > 0) {
			const message = this.bindingCalls.shift();
			if (message) {
				this.routeBindingCall(message).catch((error: unknown) => {
					console.warn("[Codex Helper] bridge route failed", error);
				});
			}
		}
	}

	private async routeBindingCall(message: CdpMessage): Promise<void> {
		const payloadText = message.params?.payload;
		if (typeof payloadText !== "string") return;

		let parsed: Record<string, JsonValue>;
		try {
			parsed = JSON.parse(payloadText) as Record<string, JsonValue>;
		} catch {
			return;
		}

		const requestId = stringValue(parsed.id);
		const path = stringValue(parsed.path);
		const payload = (parsed.payload ?? {}) as Record<string, JsonValue>;
		if (!requestId) return;

		try {
			const result = await withBridgeRequestTimeout(
				handleBridgeRequest(path, payload),
				path,
			);
			const expression = `window.__codexHelperResolve(${JSON.stringify(requestId)}, ${JSON.stringify(result)})`;
			await this.sendCommandWithoutWait(
				nextMessageId++,
				"Runtime.evaluate",
				runtimeEvaluateParams(expression),
			);
		} catch (error) {
			const messageText =
				error instanceof Error ? error.message : String(error);
			const expression = `window.__codexHelperReject(${JSON.stringify(requestId)}, ${JSON.stringify(messageText)})`;
			await this.sendCommandWithoutWait(
				nextMessageId++,
				"Runtime.evaluate",
				runtimeEvaluateParams(expression),
			);
		}
	}
}

function stringValue(value: unknown): string {
	return typeof value === "string" ? value : "";
}

export function loadRuntimeScripts(paths: string[]): string[] {
	return paths.map((path) => readFileSync(path, "utf8"));
}

export function buildRuntimeBundle(paths: string[]): string {
	return loadRuntimeScripts(paths).join("\n;\n");
}

export async function installBridge(options: {
	debugPort: number;
	targetId: string;
	runtimeScripts: string[];
	timer: LaunchTimer;
}): Promise<() => void> {
	const { timer } = options;
	timer.stage("inject start", {
		targetId: options.targetId,
		scriptCount: options.runtimeScripts.length,
	});
	const browserWsUrl = await browserWebsocketUrl(options.debugPort);
	timer.stage("inject browser websocket url ready");

	const socket = new WebSocket(browserWsUrl);
	await new Promise<void>((resolve, reject) => {
		socket.addEventListener("open", () => resolve(), { once: true });
		socket.addEventListener(
			"error",
			() =>
				reject(
					new Error(`Failed to connect CDP browser websocket: ${browserWsUrl}`),
				),
			{ once: true },
		);
	});
	timer.stage("inject websocket open");

	const session = new BindingCdpSession(socket);
	const attached = await session.sendCommand(1, "Target.attachToTarget", {
		targetId: options.targetId,
		flatten: true,
	});
	const sessionId = stringValue(
		(attached.result as Record<string, JsonValue> | undefined)?.sessionId,
	);
	if (!sessionId) {
		throw new Error("CDP attach response did not include sessionId");
	}
	session.withSessionId(sessionId);
	timer.stage("inject target attached", { sessionId });

	await session.sendCommand(2, "Runtime.enable", {});
	await session.sendCommand(3, "Runtime.removeBinding", {
		name: BRIDGE_BINDING_NAME,
	});
	await session.sendCommand(4, "Runtime.addBinding", {
		name: BRIDGE_BINDING_NAME,
	});
	timer.stage("inject binding registered");

	const bridgeScript = buildBridgeScript(BRIDGE_BINDING_NAME);
	await session.sendCommand(5, "Page.addScriptToEvaluateOnNewDocument", {
		source: bridgeScript,
	});
	await session.sendCommand(
		6,
		"Runtime.evaluate",
		runtimeEvaluateParams(bridgeScript),
	);
	timer.stage("inject bridge script");

	const runtimeBytes = options.runtimeScripts.reduce(
		(total, script) => total + script.length,
		0,
	);
	for (const script of options.runtimeScripts) {
		await session.sendCommand(
			nextMessageId++,
			"Page.addScriptToEvaluateOnNewDocument",
			{
				source: script,
			},
		);
		await session.sendCommand(
			nextMessageId++,
			"Runtime.evaluate",
			runtimeEvaluateParams(script),
		);
	}
	timer.stage("inject runtime scripts", {
		scriptCount: options.runtimeScripts.length,
		bytes: runtimeBytes,
	});

	const pump = async () => {
		while (socket.readyState === WebSocket.OPEN) {
			await session.drainBindingQueue();
			await Bun.sleep(10);
		}
	};
	void pump();
	timer.stage("inject binding pump");

	return () => {
		socket.close();
	};
}
