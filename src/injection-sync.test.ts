import { expect, test } from "bun:test";

import {
	applyTargetDiscoveryMessage,
	createSerializedSyncRunner,
	disconnectInjectedTargets,
	planInjectionSync,
	runTargetWatcher,
	syncInjectedTargetsForTargets,
	type InjectedTarget,
} from "./injection-sync";
import type { CdpTarget } from "./cdp";

function target(id: string): CdpTarget {
	return {
		id,
		type: "page",
		title: "Codex",
		url: "app://-/index.html",
		webSocketDebuggerUrl: `ws://${id}`,
	};
}

function timer() {
	return {
		stage: () => {},
	};
}

test("injection sync plans inject retain and prune targets", () => {
	expect(planInjectionSync(["target-a", "target-b"], ["target-a", "old"])).toEqual(
		{
			inject: ["target-b"],
			retain: ["target-a"],
			prune: ["old"],
		},
	);
});

test("injection sync injects new targets and disconnects pruned targets", async () => {
	const disconnected: string[] = [];
	const injectedTargets = new Map<string, InjectedTarget>([
		["existing", { disconnect: () => disconnected.push("existing") }],
		["old", { disconnect: () => disconnected.push("old") }],
	]);
	const installed: string[] = [];

	await syncInjectedTargetsForTargets({
		targets: [target("existing"), target("new")],
		injectedTargets,
		timer: timer(),
		installTarget: async (currentTarget) => {
			installed.push(currentTarget.id);
			return () => disconnected.push(currentTarget.id);
		},
	});

	expect(installed).toEqual(["new"]);
	expect(disconnected).toEqual(["old"]);
	expect(Array.from(injectedTargets.keys()).sort()).toEqual(["existing", "new"]);
});

test("injection sync drops targets destroyed while install is pending", async () => {
	const destroyedTargetIds = new Set<string>();
	const disconnected: string[] = [];
	let finishInstall: (() => void) | undefined;
	const injectedTargets = new Map<string, InjectedTarget>();
	const sync = syncInjectedTargetsForTargets({
		targets: [target("closing")],
		injectedTargets,
		timer: timer(),
		shouldRetainInstalledTarget: (targetId) =>
			!destroyedTargetIds.has(targetId),
		installTarget: async (currentTarget) => {
			await new Promise<void>((resolve) => {
				finishInstall = resolve;
			});
			return () => disconnected.push(currentTarget.id);
		},
	});

	destroyedTargetIds.add("closing");
	finishInstall?.();
	await sync;

	expect(injectedTargets.has("closing")).toBe(false);
	expect(disconnected).toEqual(["closing"]);
});

test("target discovery events queue Codex page target sync", async () => {
	const injectedTargets = new Map<string, InjectedTarget>();
	let queued = 0;

	await applyTargetDiscoveryMessage({
		message: {
			method: "Target.targetInfoChanged",
			params: {
				targetInfo: {
					targetId: "created",
					type: "page",
					title: "Codex",
					url: "app://-/index.html",
				},
			},
		},
		injectedTargets,
		timer: timer(),
		queueSyncTargets: () => {
			queued += 1;
		},
	});

	expect(queued).toBe(1);
	expect(injectedTargets.has("created")).toBe(false);
});

test("target discovery events queue Codex pages before title settles", async () => {
	const injectedTargets = new Map<string, InjectedTarget>();
	let queued = 0;

	await applyTargetDiscoveryMessage({
		message: {
			method: "Target.targetInfoChanged",
			params: {
				targetInfo: {
					targetId: "created",
					type: "page",
					title: "app://-/index.html",
					url: "app://-/index.html",
				},
			},
		},
		injectedTargets,
		timer: timer(),
		queueSyncTargets: () => {
			queued += 1;
		},
	});

	expect(queued).toBe(1);
	expect(injectedTargets.has("created")).toBe(false);
});

test("target discovery events do not prune existing targets", async () => {
	const disconnected: string[] = [];
	let queued = 0;
	const injectedTargets = new Map<string, InjectedTarget>([
		["existing", { disconnect: () => disconnected.push("existing") }],
	]);

	await applyTargetDiscoveryMessage({
		message: {
			method: "Target.targetCreated",
			params: {
				targetInfo: {
					targetId: "new",
					type: "page",
					title: "Codex",
					url: "app://-/index.html",
				},
			},
		},
		injectedTargets,
		timer: timer(),
		queueSyncTargets: () => {
			queued += 1;
		},
	});

	expect(queued).toBe(1);
	expect(disconnected).toEqual([]);
	expect(Array.from(injectedTargets.keys())).toEqual(["existing"]);
});

test("target discovery tab events queue target sync", async () => {
	const injectedTargets = new Map<string, InjectedTarget>();
	let queued = 0;

	await applyTargetDiscoveryMessage({
		message: {
			method: "Target.targetCreated",
			params: {
				targetInfo: {
					targetId: "tab",
					type: "tab",
					title: "",
					url: "",
				},
			},
		},
		injectedTargets,
		timer: timer(),
		queueSyncTargets: () => {
			queued += 1;
		},
	});

	expect(queued).toBe(1);
});

test("target destroyed events disconnect injected targets", async () => {
	const disconnected: string[] = [];
	const destroyedTargetIds = new Set<string>();
	let queued = 0;
	const injectedTargets = new Map<string, InjectedTarget>([
		["closed", { disconnect: () => disconnected.push("closed") }],
	]);

	await applyTargetDiscoveryMessage({
		message: {
			method: "Target.targetDestroyed",
			params: { targetId: "closed" },
		},
		injectedTargets,
		destroyedTargetIds,
		queueSyncTargets: () => {
			queued += 1;
		},
		timer: timer(),
	});

	expect(disconnected).toEqual(["closed"]);
	expect(injectedTargets.has("closed")).toBe(false);
	expect(destroyedTargetIds.has("closed")).toBe(true);
	expect(queued).toBe(1);
});

test("disconnect injected targets clears all active bindings", () => {
	const disconnected: string[] = [];
	const injectedTargets = new Map<string, InjectedTarget>([
		["target-a", { disconnect: () => disconnected.push("target-a") }],
		["target-b", { disconnect: () => disconnected.push("target-b") }],
	]);

	const count = disconnectInjectedTargets(injectedTargets);

	expect(count).toBe(2);
	expect(disconnected.sort()).toEqual(["target-a", "target-b"]);
	expect(injectedTargets.size).toBe(0);
});

test("serialized sync runner coalesces overlapping sync requests", async () => {
	const releases: Array<() => void> = [];
	let calls = 0;
	const runner = createSerializedSyncRunner({
		syncTargets: () =>
			new Promise<void>((resolve) => {
				calls += 1;
				releases.push(resolve);
			}),
	});

	const first = runner();
	const second = runner();

	expect(first).toBe(second);
	expect(calls).toBe(1);
	releases[0]?.();
	await Bun.sleep(0);
	expect(calls).toBe(2);
	releases[1]?.();
	await first;
	expect(calls).toBe(2);
});

class FakeSocket extends EventTarget {
	readonly sent: unknown[] = [];

	send(message: string): void {
		this.sent.push(JSON.parse(message));
	}

	close(): void {
		this.dispatchEvent(new Event("close"));
	}
}

async function sendDiscoveryAckAndTargetEvent(
	socket: FakeSocket,
	promise: Promise<void>,
): Promise<void> {
	socket.dispatchEvent(
		new MessageEvent("message", {
			data: JSON.stringify({ id: 1, result: {} }),
		}),
	);
	socket.dispatchEvent(
		new MessageEvent("message", {
			data: JSON.stringify({
				method: "Target.targetInfoChanged",
				params: {
					targetInfo: {
						targetId: "target-a",
						type: "page",
						title: "Codex",
						url: "app://-/index.html",
					},
				},
			}),
		}),
	);
	await Bun.sleep(0);
	socket.close();
	await promise;
}

test("target watcher rejects when socket closes before discovery ack", async () => {
	const socket = new FakeSocket();
	const promise = runTargetWatcher({
		socket: socket as unknown as WebSocket,
		injectedTargets: new Map<string, InjectedTarget>(),
		syncTargets: async () => {},
		queueSyncTargets: () => {},
		timer: timer(),
		stopped: () => false,
	});

	socket.close();

	await expect(promise).rejects.toThrow(
		"Target.setDiscoverTargets socket closed before response",
	);
});

test("target watcher suppresses target event logs by default", async () => {
	const socket = new FakeSocket();
	const stages: string[] = [];
	const promise = runTargetWatcher({
		socket: socket as unknown as WebSocket,
		injectedTargets: new Map<string, InjectedTarget>(),
		syncTargets: async () => {},
		queueSyncTargets: () => {},
		timer: {
			stage: (name) => stages.push(name),
		},
		stopped: () => false,
	});

	await sendDiscoveryAckAndTargetEvent(socket, promise);

	expect(stages).toEqual(["target watcher ready"]);
});

test("target watcher logs target events when debug events are enabled", async () => {
	const socket = new FakeSocket();
	const stages: string[] = [];
	const promise = runTargetWatcher({
		socket: socket as unknown as WebSocket,
		injectedTargets: new Map<string, InjectedTarget>(),
		syncTargets: async () => {},
		queueSyncTargets: () => {},
		debugTargetEvents: true,
		timer: {
			stage: (name) => stages.push(name),
		},
		stopped: () => false,
	});

	await sendDiscoveryAckAndTargetEvent(socket, promise);

	expect(stages).toEqual(["target event", "target watcher ready"]);
});
