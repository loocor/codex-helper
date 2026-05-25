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

function repoRoot(): string {
	return join(import.meta.dir, "..");
}

function bridgeBinaryName(): string {
	return process.platform === "win32"
		? "codex-helper-bridge.exe"
		: "codex-helper-bridge";
}

function absolutePath(path: string): boolean {
	return (
		path.startsWith("/") ||
		path.startsWith("\\\\") ||
		path.startsWith("//") ||
		/^[A-Za-z]:[\\/]/.test(path)
	);
}

function configuredBridgeBinaryPath(root = repoRoot()): string | null {
	const configured = process.env.CODEX_HELPER_BRIDGE_BIN?.trim();
	if (!configured) return null;
	return absolutePath(configured) ? configured : join(root, configured);
}

export function rustBridgeCandidatePaths(root = repoRoot()): string[] {
	const binaryName = bridgeBinaryName();
	const candidates: string[] = [];
	const configured = configuredBridgeBinaryPath(root);
	if (configured) candidates.push(configured);
	const cargoTargetDir = process.env.CARGO_TARGET_DIR?.trim();
	if (cargoTargetDir) {
		const targetRoot = absolutePath(cargoTargetDir)
			? cargoTargetDir
			: join(root, cargoTargetDir);
		candidates.push(
			join(targetRoot, "debug", binaryName),
			join(targetRoot, "release", binaryName),
		);
	}
	candidates.push(
		join(root, "src-tauri", "target", "debug", binaryName),
		join(root, "src-tauri", "target", "release", binaryName),
		join(root, "target", "debug", binaryName),
		join(root, "target", "release", binaryName),
	);
	return Array.from(new Set(candidates));
}

function shellQuote(value: string): string {
	return `'${value.replaceAll("'", "'\\''")}'`;
}

function rustBridgeBuildCommand(root = repoRoot()): string {
	const manifestPath = shellQuote(join(root, "src-tauri", "Cargo.toml"));
	if (process.platform === "win32") {
		return [
			"cargo build",
			`--manifest-path ${manifestPath}`,
			"--bin codex-helper-bridge",
		].join(" ");
	}
	return [
		"env RUSTC_WRAPPER=",
		"cargo build",
		`--manifest-path ${manifestPath}`,
		"--bin codex-helper-bridge",
	].join(" ");
}

export function rustBridgeBinaryPath(root = repoRoot()): string {
	const configured = configuredBridgeBinaryPath(root);
	const candidates = rustBridgeCandidatePaths(root);
	if (configured && !existsSync(configured)) {
		throw new Error(
			`Configured Rust bridge binary not found: ${configured}. Build it with: ${rustBridgeBuildCommand(root)}`,
		);
	}
	for (const binary of candidates) {
		if (existsSync(binary)) return binary;
	}
	throw new Error(
		`Rust bridge binary not found. Build it with: ${rustBridgeBuildCommand(root)}. Checked: ${candidates.join(", ")}`,
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
