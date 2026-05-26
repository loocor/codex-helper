import { installBridge } from "./bridge";
import {
	ALL_TARGETS_FILTER,
	browserWebsocketUrl,
	codexInjectablePageTargets,
	hasCodexCdpTarget,
	isCodexPageTarget,
	listBrowserTargets,
	type CdpTarget,
} from "./cdp";
import type { LaunchTimer } from "./debug";

export type InjectedTarget = {
	disconnect: () => void;
};

export type InjectionSyncPlan = {
	inject: string[];
	retain: string[];
	prune: string[];
};

export type InjectionSyncResult = InjectionSyncPlan & {
	failures: string[];
};

type InstallTarget = (target: CdpTarget) => Promise<() => void>;

type TargetInfo = {
	targetId?: string;
	type?: string;
	title?: string;
	url?: string;
};

type TargetDiscoveryMessage = {
	id?: number;
	method?: string;
	error?: unknown;
	params?: {
		targetId?: string;
		targetInfo?: TargetInfo;
	};
};

const TARGET_WATCHER_RECONNECT_MS = 1000;
const TARGET_WATCHER_DISCOVERY_TIMEOUT_MS = 5000;
const TARGET_SYNC_DELAYS_MS = [250, 1000];

export function planInjectionSync(
	currentIds: string[],
	injectedIds: string[],
): InjectionSyncPlan {
	const current = new Set(currentIds);
	const injected = new Set(injectedIds);
	return {
		inject: currentIds.filter((id) => !injected.has(id)),
		retain: currentIds.filter((id) => injected.has(id)),
		prune: injectedIds.filter((id) => !current.has(id)),
	};
}

export async function syncInjectedTargetsForTargets(options: {
	targets: CdpTarget[];
	injectedTargets: Map<string, InjectedTarget>;
	installTarget: InstallTarget;
	shouldRetainInstalledTarget?: (targetId: string) => boolean;
	timer: LaunchTimer;
}): Promise<InjectionSyncResult> {
	const currentIds = options.targets.map((target) => target.id);
	const plan = planInjectionSync(
		currentIds,
		Array.from(options.injectedTargets.keys()),
	);
	const failures: string[] = [];

	for (const targetId of plan.inject) {
		const target = options.targets.find((candidate) => candidate.id === targetId);
		if (!target) continue;
		try {
			const disconnect = await options.installTarget(target);
			if (options.shouldRetainInstalledTarget?.(target.id) === false) {
				disconnect();
				continue;
			}
			options.injectedTargets.set(target.id, { disconnect });
		} catch (error) {
			const message = error instanceof Error ? error.message : String(error);
			failures.push(`${target.id}: ${message}`);
			options.timer.stage("inject target failed", { targetId: target.id, message });
		}
	}

	for (const targetId of plan.prune) {
		const injected = options.injectedTargets.get(targetId);
		if (!injected) continue;
		injected.disconnect();
		options.injectedTargets.delete(targetId);
	}

	if (plan.inject.length > 0 || plan.prune.length > 0 || failures.length > 0) {
		options.timer.stage("sync targets", {
			discovered: options.targets.length,
			injected: plan.inject.length - failures.length,
			retained: plan.retain.length,
			pruned: plan.prune.length,
			failures: failures.join("; "),
			targetIds: currentIds.join(","),
		});
	}

	return { ...plan, failures };
}

export function disconnectInjectedTargets(
	injectedTargets: Map<string, InjectedTarget>,
): number {
	const count = injectedTargets.size;
	for (const injected of injectedTargets.values()) injected.disconnect();
	injectedTargets.clear();
	return count;
}

export async function syncInjectedTargets(options: {
	debugPort: number;
	runtimeScripts: string[];
	injectedTargets: Map<string, InjectedTarget>;
	shouldRetainInstalledTarget?: (targetId: string) => boolean;
	timer: LaunchTimer;
	createHelperInstanceId: () => string;
}): Promise<InjectionSyncResult> {
	const targets = codexInjectablePageTargets(
		await listBrowserTargets(options.debugPort),
	);
	const syncOptions: Parameters<typeof syncInjectedTargetsForTargets>[0] = {
		targets,
		injectedTargets: options.injectedTargets,
		timer: options.timer,
		installTarget: (target) =>
			installBridge({
				debugPort: options.debugPort,
				targetId: target.id,
				helperInstanceId: options.createHelperInstanceId(),
				runtimeScripts: options.runtimeScripts,
				timer: options.timer,
			}),
	};
	if (options.shouldRetainInstalledTarget) {
		syncOptions.shouldRetainInstalledTarget = options.shouldRetainInstalledTarget;
	}
	return syncInjectedTargetsForTargets(syncOptions);
}

function targetFromInfo(targetInfo: TargetInfo): CdpTarget | null {
	if (!targetInfo.targetId || !targetInfo.type) return null;
	return {
		id: targetInfo.targetId,
		type: targetInfo.type,
		title: targetInfo.title ?? "",
		url: targetInfo.url ?? "",
	};
}

function isCodexTabTargetInfo(target: CdpTarget): boolean {
	return target.type === "tab";
}

export async function applyTargetDiscoveryMessage(options: {
	message: TargetDiscoveryMessage;
	injectedTargets: Map<string, InjectedTarget>;
	destroyedTargetIds?: Set<string>;
	queueSyncTargets?: () => void;
	timer: LaunchTimer;
}): Promise<void> {
	if (
		options.message.method === "Target.targetCreated" ||
		options.message.method === "Target.targetInfoChanged"
	) {
		const target = targetFromInfo(options.message.params?.targetInfo ?? {});
		if (!target) return;
		if (isCodexTabTargetInfo(target) || isCodexPageTarget(target)) {
			options.queueSyncTargets?.();
		}
		return;
	}

	if (options.message.method === "Target.targetDestroyed") {
		const targetId = options.message.params?.targetId;
		if (!targetId) return;
		options.destroyedTargetIds?.add(targetId);
		const injected = options.injectedTargets.get(targetId);
		if (injected) {
			injected.disconnect();
			options.injectedTargets.delete(targetId);
			options.timer.stage("sync targets", {
				discovered: 0,
				injected: 0,
				retained: options.injectedTargets.size,
				pruned: 1,
				failures: "",
				targetIds: Array.from(options.injectedTargets.keys()).join(","),
			});
		}
		options.queueSyncTargets?.();
	}
}

export function createSerializedSyncRunner(options: {
	syncTargets: () => Promise<void>;
}): () => Promise<void> {
	let active: Promise<void> | undefined;
	let rerun = false;

	const run = async (): Promise<void> => {
		while (true) {
			rerun = false;
			await options.syncTargets();
			if (!rerun) return;
		}
	};

	return () => {
		if (active) {
			rerun = true;
			return active;
		}
		active = run().finally(() => {
			active = undefined;
		});
		return active;
	};
}

async function openBrowserSocket(websocketUrl: string): Promise<WebSocket> {
	const socket = new WebSocket(websocketUrl);
	await new Promise<void>((resolve, reject) => {
		socket.addEventListener("open", () => resolve(), { once: true });
		socket.addEventListener(
			"error",
			() =>
				reject(
					new Error(`Failed to connect CDP browser websocket: ${websocketUrl}`),
				),
			{ once: true },
		);
	});
	return socket;
}

export async function runTargetWatcher(options: {
	socket: WebSocket;
	injectedTargets: Map<string, InjectedTarget>;
	syncTargets: () => Promise<void>;
	queueSyncTargets: () => void;
	destroyedTargetIds?: Set<string>;
	debugTargetEvents?: boolean;
	timer: LaunchTimer;
	stopped: () => boolean;
}): Promise<void> {
	const discoveryCommandId = 1;
	const discoveryReady = new Promise<void>((resolve, reject) => {
		let settled = false;
		const timeout = setTimeout(() => {
			fail(
				new Error(
					`Target.setDiscoverTargets timed out after ${TARGET_WATCHER_DISCOVERY_TIMEOUT_MS}ms`,
				),
			);
		}, TARGET_WATCHER_DISCOVERY_TIMEOUT_MS);
		const cleanup = () => {
			clearTimeout(timeout);
			options.socket.removeEventListener("close", onClose);
			options.socket.removeEventListener("error", onError);
		};
		const done = () => {
			if (settled) return;
			settled = true;
			cleanup();
			resolve();
		};
		const fail = (error: Error) => {
			if (settled) return;
			settled = true;
			cleanup();
			reject(error);
		};
		const onClose = () =>
			fail(new Error("Target.setDiscoverTargets socket closed before response"));
		const onError = () =>
			fail(new Error("Target.setDiscoverTargets socket errored before response"));
		options.socket.addEventListener("close", onClose, { once: true });
		options.socket.addEventListener("error", onError, { once: true });
		options.socket.addEventListener("message", (event) => {
			const message = JSON.parse(String(event.data)) as TargetDiscoveryMessage;
			if (message.id === discoveryCommandId) {
				if (message.error) {
					fail(
						new Error(
							`Target.setDiscoverTargets failed: ${JSON.stringify(message.error)}`,
						),
					);
				} else {
					done();
				}
				return;
			}
			if (options.stopped()) return;
			if (options.debugTargetEvents && message.method?.startsWith("Target.")) {
				options.timer.stage("target event", {
					method: message.method,
					targetId:
						message.params?.targetId ?? message.params?.targetInfo?.targetId ?? "",
					type: message.params?.targetInfo?.type ?? "",
					url: message.params?.targetInfo?.url ?? "",
				});
			}
			const discoveryOptions: Parameters<typeof applyTargetDiscoveryMessage>[0] = {
				message,
				injectedTargets: options.injectedTargets,
				queueSyncTargets: options.queueSyncTargets,
				timer: options.timer,
			};
			if (options.destroyedTargetIds) {
				discoveryOptions.destroyedTargetIds = options.destroyedTargetIds;
			}
			applyTargetDiscoveryMessage(discoveryOptions).catch((error: unknown) => {
				options.timer.stage("target event failed", {
					message: error instanceof Error ? error.message : String(error),
				});
			});
		});
	});
	options.socket.send(
		JSON.stringify({
			id: discoveryCommandId,
			method: "Target.setDiscoverTargets",
			params: { discover: true, filter: ALL_TARGETS_FILTER },
		}),
	);
	await discoveryReady;
	options.timer.stage("target watcher ready");
	await new Promise<void>((resolve) => {
		options.socket.addEventListener("close", () => resolve(), { once: true });
		options.socket.addEventListener("error", () => resolve(), { once: true });
	});
}

export function startCodexTargetWatcher(options: {
	debugPort: number;
	runtimeScripts: string[];
	injectedTargets: Map<string, InjectedTarget>;
	timer: LaunchTimer;
	createHelperInstanceId: () => string;
	onCodexDisconnected?: () => void;
	debugTargetEvents?: boolean;
}): () => void {
	let stopped = false;
	let codexOnline = true;
	let socket: WebSocket | undefined;
	const destroyedTargetIds = new Set<string>();
	const queuedSyncTimers = new Map<number, ReturnType<typeof setTimeout>>();
	const syncTargets = () =>
		syncInjectedTargets({
			debugPort: options.debugPort,
			runtimeScripts: options.runtimeScripts,
			injectedTargets: options.injectedTargets,
			shouldRetainInstalledTarget: (targetId) => !destroyedTargetIds.has(targetId),
			timer: options.timer,
			createHelperInstanceId: options.createHelperInstanceId,
		}).then(() => {});
	const runSyncTargets = createSerializedSyncRunner({ syncTargets });
	const queueSyncTargets = () => {
		for (const delayMs of TARGET_SYNC_DELAYS_MS) {
			if (queuedSyncTimers.has(delayMs)) continue;
			const timer = setTimeout(() => {
				queuedSyncTimers.delete(delayMs);
				if (stopped) return;
				runSyncTargets().catch((error: unknown) => {
					options.timer.stage("target sync failed", {
						message: error instanceof Error ? error.message : String(error),
					});
				});
			}, delayMs);
			queuedSyncTimers.set(delayMs, timer);
		}
	};
	const run = async () => {
		while (!stopped) {
			try {
				await runSyncTargets();
				codexOnline = true;
				socket = await openBrowserSocket(
					await browserWebsocketUrl(options.debugPort),
				);
				await runTargetWatcher({
					socket,
					injectedTargets: options.injectedTargets,
					syncTargets: runSyncTargets,
					queueSyncTargets,
					destroyedTargetIds,
					debugTargetEvents: options.debugTargetEvents === true,
					timer: options.timer,
					stopped: () => stopped,
				});
			} catch (error) {
				if (!stopped) {
					const message = error instanceof Error ? error.message : String(error);
					if (codexOnline) {
						options.timer.stage("target watcher failed", { message });
					}
					if (!(await hasCodexCdpTarget(options.debugPort))) {
						if (codexOnline) {
							options.onCodexDisconnected?.();
						}
						codexOnline = false;
					}
				}
			} finally {
				socket?.close();
				socket = undefined;
			}
			if (!stopped) await Bun.sleep(TARGET_WATCHER_RECONNECT_MS);
		}
	};
	void run();
	return () => {
		stopped = true;
		for (const timer of queuedSyncTimers.values()) clearTimeout(timer);
		queuedSyncTimers.clear();
		socket?.close();
	};
}
