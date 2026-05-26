import { hasCodexCdpTarget, isDebugPortReady } from "./cdp";
import type { LaunchTimer } from "./debug";

import { isPortFree, listenPidsOnPort } from "./port";

export type PortHold = {
	port: number;
	release: () => void;
};

export function reserveEphemeralPort(): PortHold {
	const server = Bun.listen({
		hostname: "127.0.0.1",
		port: 0,
		socket: {
			data() {},
		},
	});
	const port = server.port;
	if (!port) {
		server.stop();
		throw new Error("Failed to reserve a free local TCP port");
	}
	return {
		port,
		release: () => {
			server.stop();
		},
	};
}

/** @deprecated Use reserveEphemeralPort when the port must stay reserved until Codex binds. */
export function allocateFreePort(): number {
	return reserveEphemeralPort().port;
}

export const PREFERRED_DEBUG_PORT = 9229;
export const DEBUG_PORT_SCAN_LIMIT = 32;

export type DebugPortResolution = {
	port: number;
	mode: "attach" | "launch";
	portHold?: PortHold;
};

export type ResolveDebugPortOptions = {
	preferred?: number;
	scanLimit?: number;
	explicitPort?: number | undefined;
	timer?: LaunchTimer;
};

export async function findAttachableDebugPort(
	preferred = PREFERRED_DEBUG_PORT,
	scanLimit = DEBUG_PORT_SCAN_LIMIT,
): Promise<number | null> {
	for (let offset = 0; offset < scanLimit; offset += 1) {
		const port = preferred + offset;
		if (await hasCodexCdpTarget(port)) return port;
	}
	return null;
}

export function findFreeDebugPort(
	preferred = PREFERRED_DEBUG_PORT,
	scanLimit = DEBUG_PORT_SCAN_LIMIT,
): number | null {
	for (let offset = 0; offset < scanLimit; offset += 1) {
		const port = preferred + offset;
		if (isPortFree(port)) return port;
	}
	return null;
}

export async function resolveDebugPort(
	preferred = PREFERRED_DEBUG_PORT,
	scanLimit = DEBUG_PORT_SCAN_LIMIT,
): Promise<DebugPortResolution> {
	void preferred;
	void scanLimit;
	const portHold = reserveEphemeralPort();
	return { port: portHold.port, mode: "launch", portHold };
}

export async function resolveDebugPortForLaunch(
	options: ResolveDebugPortOptions,
): Promise<DebugPortResolution> {
	const preferred = options.preferred ?? PREFERRED_DEBUG_PORT;
	const scanLimit = options.scanLimit ?? DEBUG_PORT_SCAN_LIMIT;

	if (options.explicitPort !== undefined) {
		const port = options.explicitPort;
		if (await hasCodexCdpTarget(port)) {
			return { port, mode: "attach" };
		}
		if (await isDebugPortReady(port)) {
			throw new Error(
				`Debug port ${port} exposes a browser CDP endpoint but not Codex. Stop the other app or omit --debug-port to auto-select a port.`,
			);
		}
		if (!isPortFree(port)) {
			throw new Error(
				`Debug port ${port} is in use and does not expose Codex CDP. Stop the blocking process or omit --debug-port to auto-select a port.`,
			);
		}
		return { port, mode: "launch" };
	}

	const resolved = await resolveDebugPort(preferred, scanLimit);
	options.timer?.stage("resolved debug port", {
		port: resolved.port,
		mode: resolved.mode,
		preferred,
	});
	return resolved;
}

export function isPortFreeForLaunch(port: number): boolean {
	return isPortFree(port);
}

export { listenPidsOnPort };
