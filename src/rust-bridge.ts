import { execFile } from "node:child_process";
import { existsSync } from "node:fs";
import { join } from "node:path";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);

type JsonValue =
	| null
	| boolean
	| number
	| string
	| JsonValue[]
	| { [key: string]: JsonValue };

const RUST_BRIDGE_PATHS = new Set([
	"/delete",
	"/export-markdown",
	"/move-thread-workspace",
	"/undo",
	"/backups/list",
	"/backups/restore",
]);

export function isRustBridgePath(path: string): boolean {
	return RUST_BRIDGE_PATHS.has(path);
}

export function rustBridgeBinaryPath(): string {
	const root = join(import.meta.dir, "..");
	for (const subpath of [
		"src-tauri/target/debug/codex-helper-bridge",
		"src-tauri/target/release/codex-helper-bridge",
	]) {
		const binary = join(root, subpath);
		if (existsSync(binary)) return binary;
	}
	throw new Error(
		"Rust bridge binary not found. Run: env RUSTC_WRAPPER= cargo build --manifest-path src-tauri/Cargo.toml --bin codex-helper-bridge",
	);
}

export async function invokeRustBridge(
	path: string,
	payload: Record<string, JsonValue>,
): Promise<JsonValue> {
	const binary = rustBridgeBinaryPath();
	const { stdout, stderr } = await execFileAsync(binary, [
		path,
		JSON.stringify(payload),
	]);
	const text = stdout.trim();
	if (!text) {
		const message = stderr.trim() || "Empty bridge response";
		return { status: "failed", message };
	}
	return JSON.parse(text) as JsonValue;
}
