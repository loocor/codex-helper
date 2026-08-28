import { existsSync, mkdtempSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { expect, test } from "bun:test";
import {
	resolveScreenshotPath,
	writeScreenshot,
} from "./validate-native-settings-helpers.mjs";

const source = readFileSync(
	join(import.meta.dir, "validate-native-settings.mjs"),
	"utf8",
);

test("native settings validator defaults screenshots to the system temp directory", () => {
	expect(source).toContain("resolveScreenshotPath()");
	expect(source).not.toContain("/private/tmp/codex-helper-native-settings-logs.png");

	expect(resolveScreenshotPath({})).toBe(
		join(tmpdir(), "codex-helper-native-settings-logs.png"),
	);
	expect(
		resolveScreenshotPath({ CODEX_HELPER_VALIDATE_SCREENSHOT: "/tmp/custom.png" }),
	).toBe("/tmp/custom.png");
});

test("native settings validator creates the screenshot output directory", () => {
	expect(source).toContain("writeScreenshot(");

	const root = mkdtempSync(join(tmpdir(), "codex-helper-validator-"));
	const screenshotPath = join(root, "nested", "settings.png");

	writeScreenshot(screenshotPath, Buffer.from("png").toString("base64"));

	expect(existsSync(screenshotPath)).toBe(true);
	expect(readFileSync(screenshotPath, "utf8")).toBe("png");
});

test("native settings validator follows current Helper settings pages", () => {
	expect(source).toContain('labels !== "General|Providers|Endpoint|Logs|About"');
	expect(source).toContain(
		'iconNames !== "sliders-horizontal|plug|radio|scroll-text|info"',
	);
	expect(source).toContain("function startHelperSettingsApp(");
	expect(source).not.toContain("Deleted Sessions");
	expect(source).not.toContain("deleted-sessions");
	expect(source).not.toContain("trash-2");
});
