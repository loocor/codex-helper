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
