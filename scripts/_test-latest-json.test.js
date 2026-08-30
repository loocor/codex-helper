import { expect, test } from "bun:test";
import { mkdtempSync, writeFileSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { spawnSync } from "node:child_process";

const repoRoot = resolve(import.meta.dir, "..");
const script = join(repoRoot, "scripts/generate-latest-json.sh");

test("generate-latest-json writes a complete Tauri updater manifest", () => {
  const directory = mkdtempSync(join(tmpdir(), "codex-helper-latest-json-"));
  const notesFile = join(directory, "notes.md");
  const aarch64Archive = join(directory, "CodexHelper-0.2.2-macos-aarch64.app.tar.gz");
  const x86Archive = join(directory, "CodexHelper-0.2.2-macos-x86_64.app.tar.gz");
  const aarch64Sig = join(directory, "aarch64.sig");
  const x86Sig = join(directory, "x86_64.sig");
  const output = join(directory, "latest.json");

  writeFileSync(notesFile, "## What's Changed\n* Add updater\n");
  writeFileSync(aarch64Archive, "aarch64-archive");
  writeFileSync(x86Archive, "x86-archive");
  writeFileSync(aarch64Sig, "aarch64-signature\n");
  writeFileSync(x86Sig, "x86-signature\n");

  const result = spawnSync(
    "bash",
    [
      script,
      "--version",
      "0.2.2",
      "--notes-file",
      notesFile,
      "--pub-date",
      "2026-08-30T12:00:00Z",
      "--asset-base-url",
      "https://github.com/loocor/codex-helper/releases/download/v0.2.2",
      "--aarch64-archive",
      aarch64Archive,
      "--aarch64-sig",
      aarch64Sig,
      "--x86_64-archive",
      x86Archive,
      "--x86_64-sig",
      x86Sig,
      "--output",
      output,
    ],
    { encoding: "utf8" },
  );

  expect(result.status).toBe(0);
  expect(result.stderr).toBe("");

  const manifest = JSON.parse(readFileSync(output, "utf8"));
  expect(manifest).toEqual({
    version: "0.2.2",
    notes: "## What's Changed\n* Add updater",
    pub_date: "2026-08-30T12:00:00Z",
    platforms: {
      "darwin-aarch64": {
        url: "https://github.com/loocor/codex-helper/releases/download/v0.2.2/CodexHelper-0.2.2-macos-aarch64.app.tar.gz",
        signature: "aarch64-signature",
      },
      "darwin-x86_64": {
        url: "https://github.com/loocor/codex-helper/releases/download/v0.2.2/CodexHelper-0.2.2-macos-x86_64.app.tar.gz",
        signature: "x86-signature",
      },
    },
  });
});

test("generate-latest-json fails when a platform signature is missing", () => {
  const directory = mkdtempSync(join(tmpdir(), "codex-helper-latest-json-missing-"));
  const notesFile = join(directory, "notes.md");
  writeFileSync(notesFile, "notes");
  const result = spawnSync(
    "bash",
    [
      script,
      "--version",
      "0.2.2",
      "--notes-file",
      notesFile,
      "--pub-date",
      "2026-08-30T12:00:00Z",
      "--asset-base-url",
      "https://example.invalid",
      "--aarch64-archive",
      notesFile,
      "--aarch64-sig",
      notesFile,
      "--x86_64-archive",
      notesFile,
      "--x86_64-sig",
      join(directory, "missing.sig"),
      "--output",
      join(directory, "latest.json"),
    ],
    { encoding: "utf8" },
  );
  expect(result.status).not.toBe(0);
  expect(result.stderr).toContain("missing file");
});
