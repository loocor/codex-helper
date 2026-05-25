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

test("configured relative rust bridge binary resolves against the candidate root", () => {
	const previous = process.env.CODEX_HELPER_BRIDGE_BIN;
	try {
		process.env.CODEX_HELPER_BRIDGE_BIN = "custom/bin/codex-helper-bridge";

		expect(rustBridgeCandidatePaths(root)[0]).toBe(
			join(root, "custom/bin/codex-helper-bridge"),
		);
	} finally {
		if (previous === undefined) delete process.env.CODEX_HELPER_BRIDGE_BIN;
		else process.env.CODEX_HELPER_BRIDGE_BIN = previous;
	}
});

test("configured rust bridge UNC path is treated as absolute", () => {
	const previous = process.env.CODEX_HELPER_BRIDGE_BIN;
	try {
		process.env.CODEX_HELPER_BRIDGE_BIN =
			"\\\\server\\share\\codex-helper-bridge.exe";

		expect(rustBridgeCandidatePaths(root)[0]).toBe(
			"\\\\server\\share\\codex-helper-bridge.exe",
		);
	} finally {
		if (previous === undefined) delete process.env.CODEX_HELPER_BRIDGE_BIN;
		else process.env.CODEX_HELPER_BRIDGE_BIN = previous;
	}
});

test("missing rust bridge binary message quotes manifest paths with spaces", () => {
	const previous = process.env.CODEX_HELPER_BRIDGE_BIN;
	try {
		process.env.CODEX_HELPER_BRIDGE_BIN = "missing-bridge";

		expect(() => rustBridgeBinaryPath(root)).toThrow(
			"cargo build --manifest-path '/tmp/codex-helper-root/src-tauri/Cargo.toml'",
		);
	} finally {
		if (previous === undefined) delete process.env.CODEX_HELPER_BRIDGE_BIN;
		else process.env.CODEX_HELPER_BRIDGE_BIN = previous;
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
