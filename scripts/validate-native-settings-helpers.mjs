import { mkdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";

export function resolveScreenshotPath(env = process.env) {
  return (
    env.CODEX_HELPER_VALIDATE_SCREENSHOT ||
    join(tmpdir(), "codex-helper-native-settings-logs.png")
  );
}

export function writeScreenshot(path, base64Data) {
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, Buffer.from(base64Data, "base64"));
}
