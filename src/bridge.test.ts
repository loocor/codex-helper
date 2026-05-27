import { readFileSync } from "node:fs";
import { join } from "node:path";
import { expect, test } from "bun:test";

const source = readFileSync(join(import.meta.dir, "bridge.ts"), "utf8");

test("bridge routes binding calls from websocket messages", () => {
	expect(source).toContain("this.routeBindingCall(message)");
	expect(source).not.toContain("drainBindingQueue");
	expect(source).not.toContain("void pump();");
	expect(source).not.toContain("Bun.sleep(10)");
});

test("dev bridge request includes caller identity", () => {
	expect(source).toContain("window.__codexHelperCallerBase");
	expect(source).toContain("window.__codexHelperCaller");
	expect(source).toContain("caller: window.__codexHelperCaller()");
});
