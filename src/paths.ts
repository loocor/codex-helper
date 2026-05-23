import { existsSync } from "node:fs";

export const defaultCodexAppPath = "/Applications/Codex.app";

export function resolveCodexAppPath(explicitPath?: string): string {
	const candidate = explicitPath?.trim() || defaultCodexAppPath;
	if (!existsSync(candidate)) {
		throw new Error(`Codex app not found: ${candidate}`);
	}
	return candidate;
}

export function runtimeBundlePath(): string {
	return new URL("../dist/bundle.js", import.meta.url).pathname;
}

export {
	buildRuntimeBundle,
	buildRuntimeScripts,
	runtimeModulePaths,
	standaloneRuntimeScripts,
	writeRuntimeBundle,
} from "../runtime/bundle";
