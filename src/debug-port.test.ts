import { expect, test } from "bun:test";

import {
	findFreeDebugPort,
	PREFERRED_DEBUG_PORT,
	reserveEphemeralPort,
	resolveDebugPortForLaunch,
} from "./debug-port";
import { isPortFree, listenPidsOnPortWithCommand } from "./port";

test("findFreeDebugPort skips occupied ports", () => {
	const held = reserveEphemeralPort();
	expect(isPortFree(held.port)).toBe(false);
	expect(findFreeDebugPort(held.port, 1)).toBeNull();
	held.release();
	expect(isPortFree(held.port)).toBe(true);
});

test("reserveEphemeralPort holds the port until released", () => {
	const held = reserveEphemeralPort();
	expect(held.port).toBeGreaterThan(0);
	expect(held.port).toBeLessThanOrEqual(65535);
	expect(isPortFree(held.port)).toBe(false);
	held.release();
	expect(isPortFree(held.port)).toBe(true);
});

test("default launch resolution reserves a random launch port", async () => {
	const preferred = reserveEphemeralPort();
	const preferredPort = preferred.port;
	preferred.release();

	const resolved = await resolveDebugPortForLaunch({
		preferred: preferredPort,
		scanLimit: 1,
	});
	try {
		expect(resolved.mode).toBe("launch");
		expect(resolved.portHold).toBeDefined();
		expect(isPortFree(resolved.port)).toBe(false);
	} finally {
		resolved.portHold?.release();
	}
});

test("preferred debug port range starts at 9229", () => {
	expect(PREFERRED_DEBUG_PORT).toBe(9229);
});

test("port probing surfaces command failures", () => {
	expect(() =>
		listenPidsOnPortWithCommand(9229, "codex-helper-missing-lsof"),
	).toThrow("codex-helper-missing-lsof");
});
