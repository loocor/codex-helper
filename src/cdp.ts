import type { LaunchTimer } from "./debug";

const CDP_FETCH_TIMEOUT_MS = 3000;
const CDP_POLL_PROGRESS_MS = 2000;

export type CdpTarget = {
	id: string;
	type: string;
	title?: string;
	url?: string;
	webSocketDebuggerUrl?: string;
};

export type CdpVersion = {
	webSocketDebuggerUrl: string;
};

export const CODEX_APP_URL = "app://-/index.html";

type CdpTargetInfo = {
	targetId?: string;
	type?: string;
	title?: string;
	url?: string;
};

type CdpTargetInfosResult = {
	targetInfos?: CdpTargetInfo[];
};

export const ALL_TARGETS_FILTER = [{}];

async function fetchCdp(
	url: string,
	init?: Omit<RequestInit, "signal">,
): Promise<Response> {
	return fetch(url, {
		...init,
		signal: AbortSignal.timeout(CDP_FETCH_TIMEOUT_MS),
	});
}

export async function browserWebsocketUrl(debugPort: number): Promise<string> {
	const response = await fetchCdp(`http://127.0.0.1:${debugPort}/json/version`);
	if (!response.ok) {
		throw new Error(
			`CDP version query failed: ${response.status} ${response.statusText}`,
		);
	}
	const version = (await response.json()) as CdpVersion;
	if (!version.webSocketDebuggerUrl?.trim()) {
		throw new Error("CDP browser websocket URL is empty");
	}
	return version.webSocketDebuggerUrl;
}

export async function isDebugPortReady(debugPort: number): Promise<boolean> {
	try {
		await browserWebsocketUrl(debugPort);
		return true;
	} catch {
		return false;
	}
}

export async function hasCodexCdpTarget(debugPort: number): Promise<boolean> {
	try {
		const targets = await listBrowserTargets(debugPort);
		return codexInjectablePageTargets(targets).length > 0;
	} catch {
		return false;
	}
}

export async function waitForDebugPort(
	debugPort: number,
	timer: LaunchTimer,
	timeoutMs = 60_000,
): Promise<void> {
	const startedAt = Date.now();
	let lastProgressAt = startedAt;
	while (Date.now() - startedAt < timeoutMs) {
		if (await isDebugPortReady(debugPort)) {
			timer.stage("debug port ready", {
				port: debugPort,
				waitedMs: Date.now() - startedAt,
			});
			return;
		}
		const now = Date.now();
		if (now - lastProgressAt >= CDP_POLL_PROGRESS_MS) {
			timer.stage("wait debug port", {
				port: debugPort,
				waitedMs: now - startedAt,
			});
			lastProgressAt = now;
		}
		await Bun.sleep(250);
	}
	const { describePortBlockers, listenPidsOnPort } = await import("./port");
	const blockerSummary = describePortBlockers(debugPort);
	throw new Error(
		listenPidsOnPort(debugPort).length > 0
			? `Timed out waiting for Codex debug port ${debugPort} after ${Date.now() - startedAt}ms. Port is held by: ${blockerSummary}`
			: `Timed out waiting for Codex debug port ${debugPort} after ${Date.now() - startedAt}ms`,
	);
}

export async function listTargets(debugPort: number): Promise<CdpTarget[]> {
	const response = await fetchCdp(`http://127.0.0.1:${debugPort}/json`);
	if (!response.ok) {
		throw new Error(
			`CDP target query failed: ${response.status} ${response.statusText}`,
		);
	}
	return response.json() as Promise<CdpTarget[]>;
}

export async function listBrowserTargets(
	debugPort: number,
): Promise<CdpTarget[]> {
	const result = (await cdpCommand(
		await browserWebsocketUrl(debugPort),
		"Target.getTargets",
		{ filter: ALL_TARGETS_FILTER },
	)) as CdpTargetInfosResult;

	return (result.targetInfos ?? []).flatMap((targetInfo) => {
		if (!targetInfo.targetId || !targetInfo.type) return [];
		return [
			{
				id: targetInfo.targetId,
				type: targetInfo.type,
				title: targetInfo.title ?? "",
				url: targetInfo.url ?? "",
			},
		];
	});
}

export function pickCodexPageTarget(targets: CdpTarget[]): CdpTarget {
	const pages = targets.filter(
		(target) => target.type === "page" && target.webSocketDebuggerUrl,
	);
	const codexPage = findCodexPageTarget(pages);
	const selected = codexPage ?? pages[0];
	if (!selected?.webSocketDebuggerUrl) {
		throw new Error("No injectable Codex page target found");
	}
	return selected;
}

export function isCodexPageTarget(target: CdpTarget): boolean {
	const titleAndUrl = `${target.title ?? ""} ${target.url ?? ""}`.toLowerCase();
	return (
		target.type === "page" &&
		(target.url === CODEX_APP_URL || titleAndUrl.includes("codex"))
	);
}

function hasTargetWebsocket(target: CdpTarget): boolean {
	return Boolean(target.webSocketDebuggerUrl?.trim());
}

export function codexPageTargets(targets: CdpTarget[]): CdpTarget[] {
	return targets.filter(
		(target) => isCodexPageTarget(target) && hasTargetWebsocket(target),
	);
}

export function codexInjectablePageTargets(targets: CdpTarget[]): CdpTarget[] {
	return targets.filter(isCodexPageTarget);
}

export function findCodexPageTarget(targets: CdpTarget[]): CdpTarget | null {
	return (
		targets.find(
			(target) => isCodexPageTarget(target) && hasTargetWebsocket(target),
		) ?? null
	);
}

export async function waitForCodexTarget(
	debugPort: number,
	timer: LaunchTimer,
	attempts = 120,
): Promise<CdpTarget> {
	const startedAt = Date.now();
	let lastProgressAt = startedAt;
	let lastError: unknown;
	timer.stage("wait cdp target start", {
		port: debugPort,
		maxAttempts: attempts,
	});
	for (let attempt = 0; attempt < attempts; attempt += 1) {
		try {
			const targets = await listTargets(debugPort);
			const target = pickCodexPageTarget(targets);
			timer.stage("wait cdp target done", {
				attempts: attempt + 1,
				waitedMs: Date.now() - startedAt,
				targetId: target.id,
				title: target.title ?? "",
				url: target.url ?? "",
			});
			return target;
		} catch (error) {
			lastError = error;
			const now = Date.now();
			if (now - lastProgressAt >= CDP_POLL_PROGRESS_MS) {
				timer.stage("wait cdp target polling", {
					attempt: attempt + 1,
					maxAttempts: attempts,
					waitedMs: now - startedAt,
					lastError: error instanceof Error ? error.message : String(error),
				});
				lastProgressAt = now;
			}
			await Bun.sleep(250);
		}
	}
	throw new Error(
		`Timed out waiting for Codex CDP target after ${Date.now() - startedAt}ms: ${String(lastError)}`,
	);
}

export async function waitForCodexTargets(
	debugPort: number,
	timer: LaunchTimer,
	attempts = 120,
): Promise<CdpTarget[]> {
	const startedAt = Date.now();
	let lastProgressAt = startedAt;
	let lastError: unknown;
	timer.stage("wait cdp targets start", {
		port: debugPort,
		maxAttempts: attempts,
	});
	for (let attempt = 0; attempt < attempts; attempt += 1) {
		try {
			const targets = await listBrowserTargets(debugPort);
			const codexTargets = codexInjectablePageTargets(targets);
			if (codexTargets.length > 0) {
				timer.stage("wait cdp targets done", {
					attempts: attempt + 1,
					waitedMs: Date.now() - startedAt,
					targetIds: codexTargets.map((target) => target.id).join(","),
					count: codexTargets.length,
				});
				return codexTargets;
			}
			lastError = new Error("No injectable Codex page target found");
		} catch (error) {
			lastError = error;
		}
		const now = Date.now();
		if (now - lastProgressAt >= CDP_POLL_PROGRESS_MS) {
			timer.stage("wait cdp targets polling", {
				attempt: attempt + 1,
				maxAttempts: attempts,
				waitedMs: now - startedAt,
				lastError:
					lastError instanceof Error ? lastError.message : String(lastError),
			});
			lastProgressAt = now;
		}
		await Bun.sleep(250);
	}
	throw new Error(
		`Timed out waiting for Codex CDP targets after ${Date.now() - startedAt}ms: ${String(lastError)}`,
	);
}

export async function cdpCommand(
	webSocketUrl: string,
	method: string,
	params: unknown,
): Promise<unknown> {
	const socket = new WebSocket(webSocketUrl);
	await new Promise<void>((resolve, reject) => {
		socket.addEventListener("open", () => resolve(), { once: true });
		socket.addEventListener(
			"error",
			() =>
				reject(new Error(`Failed to connect CDP websocket: ${webSocketUrl}`)),
			{ once: true },
		);
	});

	const id = 1;
	const result = await new Promise<unknown>((resolve, reject) => {
		socket.addEventListener("message", (event) => {
			const message = JSON.parse(String(event.data)) as {
				id?: number;
				error?: unknown;
				result?: unknown;
			};
			if (message.id !== id) return;
			if (message.error)
				reject(
					new Error(
						`CDP command ${method} failed: ${JSON.stringify(message.error)}`,
					),
				);
			else resolve(message.result);
		});
		socket.send(JSON.stringify({ id, method, params }));
	});
	socket.close();
	return result;
}
