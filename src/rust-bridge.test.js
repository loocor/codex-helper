import { join } from "node:path";
import { expect, test } from "bun:test";

import { rustBridgeCandidatePaths, rustBridgeBinaryPath } from "./rust-bridge.ts";

const root = "/tmp/codex-helper-root";

test("rust bridge candidates include configured cargo target directories", () => {
	const previousTargetDir = process.env.CARGO_TARGET_DIR;
	const previousBridgeBin = process.env.CODEX_HELPER_BRIDGE_BIN;
	try {
		process.env.CARGO_TARGET_DIR = "custom-target";
		delete process.env.CODEX_HELPER_BRIDGE_BIN;

		expect(rustBridgeCandidatePaths(root)).toContain(
			join(root, "custom-target", "debug", "codex-helper-bridge"),
		);
		expect(rustBridgeCandidatePaths(root)).toContain(
			join(root, "src-tauri", "target", "debug", "codex-helper-bridge"),
		);
	} finally {
		if (previousTargetDir === undefined) delete process.env.CARGO_TARGET_DIR;
		else process.env.CARGO_TARGET_DIR = previousTargetDir;
		if (previousBridgeBin === undefined) delete process.env.CODEX_HELPER_BRIDGE_BIN;
		else process.env.CODEX_HELPER_BRIDGE_BIN = previousBridgeBin;
	}
});

test("configured missing rust bridge binary reports the configured path", () => {
	const previous = process.env.CODEX_HELPER_BRIDGE_BIN;
	try {
		process.env.CODEX_HELPER_BRIDGE_BIN =
			"/tmp/codex-helper-missing-bridge-binary";

		expect(() => rustBridgeBinaryPath()).toThrow(
			"Configured Rust bridge binary not found: /tmp/codex-helper-missing-bridge-binary",
		);
	} finally {
		if (previous === undefined) delete process.env.CODEX_HELPER_BRIDGE_BIN;
		else process.env.CODEX_HELPER_BRIDGE_BIN = previous;
	}
});
