import { readFileSync } from "node:fs";
import { join } from "node:path";
import { expect, test } from "bun:test";

const source = readFileSync(join(import.meta.dir, "bridge.ts"), "utf8");

test("bridge injection starts binding pump without blocking readiness", () => {
	const beforePump = source.slice(0, source.indexOf("const pump = async () =>"));
	expect(beforePump).not.toContain("await session.drainBindingQueue();");
	expect(source).toContain("void pump();");
});
