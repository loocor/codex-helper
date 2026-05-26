import { readFileSync } from "node:fs";
import { join } from "node:path";
import { expect, test } from "bun:test";

const packageJson = JSON.parse(
	readFileSync(join(import.meta.dir, "..", "package.json"), "utf8"),
);

test("bun launch builds the Rust bridge binary before starting Codex", () => {
	expect(packageJson.scripts["build:bridge"]).toBe(
		"env RUSTC_WRAPPER= cargo build --manifest-path src-tauri/Cargo.toml --bin codex-helper-bridge",
	);
	expect(packageJson.scripts.launch).toBe(
		"bun run build:bridge && bun src/launch.ts",
	);
});

test("dev launcher injects all Codex page targets", () => {
	const source = readFileSync(join(import.meta.dir, "launch.ts"), "utf8");

	expect(source).toContain("waitForCodexTargets");
	expect(source).toContain("syncInjectedTargetsForTargets");
	expect(source).toContain("initialSync.failures.length > 0");
	expect(source).toContain("injectedTargets.size !== targets.length");
	expect(source).not.toContain("waitForCodexTarget(");
});

test("dev launcher keeps syncing Codex page target changes", () => {
	const source = readFileSync(join(import.meta.dir, "launch.ts"), "utf8");
	const syncSource = readFileSync(
		join(import.meta.dir, "injection-sync.ts"),
		"utf8",
	);

	expect(source).toContain("startCodexTargetWatcher({");
	expect(syncSource).toContain("Target.setDiscoverTargets");
	expect(syncSource).toContain("ALL_TARGETS_FILTER");
	expect(syncSource).toContain("Target.targetCreated");
	expect(syncSource).toContain("Target.targetInfoChanged");
	expect(syncSource).toContain("Target.targetDestroyed");
	expect(source).toContain("injectedTargets.clear()");
	expect(source).not.toContain("Bun.sleep(2000)");
});
