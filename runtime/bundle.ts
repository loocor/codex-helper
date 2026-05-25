import { mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import bundleModules from "./index.json";

const runtimeSrcDir = import.meta.dir;
const defaultOutputPath = join(runtimeSrcDir, "../dist/bundle.js");

const RUNTIME_HEADER = `(() => {
  if (typeof document === "undefined") return;

`;

const RUNTIME_FOOTER = `
})();
`;

const NON_MODULE_FILES = new Set(["bundle.ts", "index.json"]);
const BUILD_DATE_PLACEHOLDER = "__CODEX_HELPER_BUILD_DATE__";
const BUILD_DATE_PLACEHOLDER_LITERAL = `"${BUILD_DATE_PLACEHOLDER}"`;

function isRuntimeTestFile(fileName: string): boolean {
	return fileName.startsWith("_test");
}

function isRuntimeModuleFile(fileName: string): boolean {
	return fileName.endsWith(".js") && !isRuntimeTestFile(fileName);
}

function standaloneModulePaths(): string[] {
	const bundledNames = new Set(bundleModules);
	return readdirSync(runtimeSrcDir)
		.filter(
			(fileName) =>
				isRuntimeModuleFile(fileName) &&
				!bundledNames.has(fileName) &&
				!NON_MODULE_FILES.has(fileName),
		)
		.sort();
}

function runtimeBuildDate(): string {
	const configured = process.env.CODEX_HELPER_BUILD_DATE?.trim();
	if (configured) return configured;
	return new Date().toLocaleDateString("en-US", {
		month: "long",
		day: "numeric",
		year: "numeric",
		timeZone: "UTC",
	});
}

export function buildRuntimeBundle(): string {
	const body = bundleModules
		.map((fileName) => {
			const absolutePath = join(runtimeSrcDir, fileName);
			return readFileSync(absolutePath, "utf8").trimEnd();
		})
		.join("\n\n")
		.replaceAll(
			BUILD_DATE_PLACEHOLDER_LITERAL,
			JSON.stringify(runtimeBuildDate()),
		);
	return `${RUNTIME_HEADER}${body}${RUNTIME_FOOTER}`;
}

export function standaloneRuntimeScripts(): string[] {
	return standaloneModulePaths().map((fileName) =>
		readFileSync(join(runtimeSrcDir, fileName), "utf8").trimEnd(),
	);
}

export function buildRuntimeScripts(): string[] {
	return [buildRuntimeBundle(), ...standaloneRuntimeScripts()];
}

export function runtimeModulePaths(): string[] {
	return [
		...bundleModules.map((fileName) => join(runtimeSrcDir, fileName)),
		...standaloneModulePaths().map((fileName) => join(runtimeSrcDir, fileName)),
	];
}

export function writeRuntimeBundle(outputPath = defaultOutputPath): string {
	const bundled = buildRuntimeScripts().join("\n;\n");
	mkdirSync(dirname(outputPath), { recursive: true });
	writeFileSync(outputPath, bundled);
	return bundled;
}

if (import.meta.main) {
	writeRuntimeBundle();
	const standaloneCount = standaloneModulePaths().length;
	console.log(
		`Bundled ${bundleModules.length} runtime module(s) and ${standaloneCount} standalone script(s) into dist/bundle.js`,
	);
}
