import { expect, test } from "bun:test";
import { buildRuntimeBundle } from "./bundle.ts";

const source = buildRuntimeBundle();

test("runtime hides usage-limit overlays behind an explicit setting", () => {
  expect(source).toContain("function maintainUsageLimitBanner(");
  expect(source).toContain("function collectUsageLimitOverlays(");
  expect(source).toContain("function resolveUsageLimitOverlay(");
  expect(source).toContain("function normalizeUsageLimitText(");
  expect(source).toContain("function containsComposer(");
  expect(source).toContain("function restoreHiddenComposerHosts(");
  expect(source).toContain("hideUsageLimitBannerEnabled");
  expect(source).toContain("you're out of codex");
  expect(source).toContain("reset usage");
  expect(source).toContain('helperUsageHiddenAttribute = "data-codex-helper-usage-hidden"');
  expect(source).toContain("maintainUsageLimitBanner();");
  expect(source).toContain("current.tagName === \"ASIDE\"");
  expect(source).toContain("[data-placeholder]");
  expect(source).toContain('host?.closest(`[${helperUsageHiddenAttribute}="true"]`)');
});

test("usage-limit overlay matching normalizes curly apostrophes", () => {
  expect(source).toContain("\\u2018");
  expect(source).toContain("\\u2019");
  expect(source).toContain("you're out of codex");
});

test("usage-limit overlay hide does not claim to reset account limits", () => {
  expect(source).toContain(
    "Hide the Codex / Work usage-limit overlay without changing account limits.",
  );
  expect(source).not.toContain("consume_usage_reset");
  expect(source).not.toContain("rate-limit-reset-credits/consume");
});

test("restoreHiddenComposerHosts only restores composer hosts", () => {
  const restore = source.slice(
    source.indexOf("function restoreHiddenComposerHosts("),
    source.indexOf("function maintainUsageLimitBanner("),
  );
  expect(restore).toContain("if (!containsComposer(node)) continue;");
  expect(restore).not.toContain("hidesBannerCard");
  expect(restore).not.toContain('querySelector("aside")');
});

test("usage-limit hide skips nodes that contain the composer", () => {
  const hide = source.slice(
    source.indexOf("function hideUsageLimitOverlay("),
    source.indexOf("function restoreUsageLimitOverlays("),
  );
  const looksLikeCard = source.slice(
    source.indexOf("function elementLooksLikeUsageLimitCard("),
    source.indexOf("function looksLikeUsageLimitBackdrop("),
  );
  expect(hide).toContain("if (containsComposer(node)) return;");
  expect(looksLikeCard).toContain("if (containsComposer(node)) return false;");
});

test("usage-limit overlay scan is debounced like ports", () => {
  expect(source).toContain("function maintainUsageLimitBannerNow(");
  expect(source).toContain("let maintainUsageLimitBannerTimer = 0;");
  expect(source).toContain("if (maintainUsageLimitBannerTimer) return;");
  expect(source).toContain("if (maintainUsageLimitBannerTimer) clearTimeout(maintainUsageLimitBannerTimer);");
});
