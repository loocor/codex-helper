// Hide the Codex / Work usage-limit overlay without changing account limits.
function normalizeUsageLimitText(value) {
  return String(value || "")
    .replace(/[\u2018\u2019]/g, "'")
    .replace(/\s+/g, " ")
    .trim();
}

const USAGE_LIMIT_TITLE_RE =
  /you're out of codex(?: and work)? usage|you're out of codex messages|you've (?:used all|hit your) (?:codex and work usage|usage(?: limit)?|weekly limit)|已用尽\s*codex(?:\s*和\s*work)?\s*用量|codex\s*消息限额已用尽|codex(?:\s*和\s*work)?\s*用量已用尽/i;
const USAGE_LIMIT_BODY_RE =
  /your rate limit resets on|upgrade or use one of your rate limit resets|用量将于|重置后可继续|rate limit resets/i;
const USAGE_LIMIT_ACTION_RE =
  /^(upgrade|reset usage|view usage|升级|重置用量)$/i;
const USAGE_LIMIT_COMPOSER_SELECTOR =
  "textarea, [contenteditable='true'], [contenteditable=''], [data-placeholder]";

function isUsageLimitTitleText(value) {
  return USAGE_LIMIT_TITLE_RE.test(normalizeUsageLimitText(value));
}

function usageLimitBannerHasActions(node) {
  return Array.from(
    node.querySelectorAll("button, a, [role='button']"),
  ).some((action) => USAGE_LIMIT_ACTION_RE.test(normalizeUsageLimitText(textOf(action))));
}

function isHelperOrNativeSettingsTree(node) {
  return Boolean(
    node.closest(`[${helperNativeSettingsPageAttribute}]`) ||
      node.closest(`#${helperSettingsPanelId}`),
  );
}

function containsComposer(node) {
  if (!(node instanceof HTMLElement)) return false;
  return Boolean(node.querySelector(USAGE_LIMIT_COMPOSER_SELECTOR));
}

function elementLooksLikeUsageLimitCard(node) {
  if (!(node instanceof HTMLElement)) return false;
  if (isHelperOrNativeSettingsTree(node)) return false;
  if (node.getAttribute(helperUsageHiddenAttribute) === "true") return false;
  if (node.querySelector(`[${helperUsageHiddenAttribute}="true"]`)) return false;
  if (containsComposer(node)) return false;
  const text = normalizeUsageLimitText(textOf(node));
  if (!isUsageLimitTitleText(text)) return false;
  if (!(USAGE_LIMIT_BODY_RE.test(text) || usageLimitBannerHasActions(node))) {
    return false;
  }
  const rect = node.getBoundingClientRect();
  if (rect.width <= 0 || rect.height <= 0) return false;
  const role = (node.getAttribute("role") || "").toLowerCase();
  const isDialog =
    role === "dialog" || node.getAttribute("aria-modal") === "true";
  if (!isDialog) {
    if (rect.height > window.innerHeight * 0.55) return false;
    if (rect.width > window.innerWidth * 0.92) return false;
  }
  return true;
}

function looksLikeUsageLimitBackdrop(node) {
  if (!(node instanceof HTMLElement)) return false;
  if (containsComposer(node)) return false;
  const style = window.getComputedStyle(node);
  if (style.position !== "fixed" && style.position !== "absolute") return false;
  const rect = node.getBoundingClientRect();
  if (
    rect.width < window.innerWidth * 0.8 ||
    rect.height < window.innerHeight * 0.8
  ) {
    return false;
  }
  return normalizeUsageLimitText(textOf(node)).length < 8;
}

function resolveUsageLimitOverlay(card) {
  if (containsComposer(card)) return card;
  let current = card.parentElement;
  while (
    current &&
    current !== document.body &&
    current !== document.documentElement
  ) {
    if (containsComposer(current)) break;
    const role = (current.getAttribute("role") || "").toLowerCase();
    if (role === "dialog" || current.getAttribute("aria-modal") === "true") {
      return current;
    }
    const style = window.getComputedStyle(current);
    const rect = current.getBoundingClientRect();
    if (
      (style.position === "fixed" || style.position === "absolute") &&
      rect.width >= window.innerWidth * 0.75 &&
      rect.height >= window.innerHeight * 0.6
    ) {
      return current;
    }
    current = current.parentElement;
  }
  const parent = card.parentElement;
  if (
    parent instanceof HTMLElement &&
    parent !== document.body &&
    !containsComposer(parent) &&
    Array.from(parent.children).some(looksLikeUsageLimitBackdrop)
  ) {
    return parent;
  }
  return card;
}

function findUsageLimitCardFrom(start) {
  let current = start instanceof HTMLElement ? start : start?.parentElement;
  let innermost = null;
  let aside = null;
  while (
    current &&
    current !== document.body &&
    current !== document.documentElement
  ) {
    if (elementLooksLikeUsageLimitCard(current)) {
      if (!innermost) innermost = current;
      if (current.tagName === "ASIDE") aside = current;
    }
    current = current.parentElement;
  }
  return aside || innermost;
}

function collectUsageLimitOverlays() {
  const overlays = new Set();
  const root = document.body || document.documentElement;
  if (!root) return overlays;
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  while (walker.nextNode()) {
    if (!isUsageLimitTitleText(walker.currentNode.nodeValue || "")) continue;
    const host = walker.currentNode.parentElement;
    if (host?.closest(`[${helperUsageHiddenAttribute}="true"]`)) continue;
    const card = findUsageLimitCardFrom(host);
    if (!card) continue;
    overlays.add(resolveUsageLimitOverlay(card));
  }
  return overlays;
}

function hideUsageLimitOverlay(node) {
  if (!(node instanceof HTMLElement)) return;
  if (containsComposer(node)) return;
  if (node.getAttribute(helperUsageHiddenAttribute) === "true") return;
  node.setAttribute(helperUsageHiddenAttribute, "true");
  node.hidden = true;
  node.setAttribute("aria-hidden", "true");
}

function restoreUsageLimitOverlays() {
  for (const node of document.querySelectorAll(
    `[${helperUsageHiddenAttribute}="true"]`,
  )) {
    if (!(node instanceof HTMLElement)) continue;
    node.hidden = false;
    node.removeAttribute("aria-hidden");
    node.removeAttribute(helperUsageHiddenAttribute);
  }
}

function restoreHiddenComposerHosts() {
  for (const node of document.querySelectorAll(
    `[${helperUsageHiddenAttribute}="true"]`,
  )) {
    if (!(node instanceof HTMLElement)) continue;
    if (!containsComposer(node)) continue;
    node.hidden = false;
    node.removeAttribute("aria-hidden");
    node.removeAttribute(helperUsageHiddenAttribute);
  }
}

function maintainUsageLimitBanner() {
  if (maintainUsageLimitBannerTimer) return;
  maintainUsageLimitBannerTimer = window.setTimeout(() => {
    maintainUsageLimitBannerTimer = 0;
    maintainUsageLimitBannerNow();
  }, 150);
}

function maintainUsageLimitBannerNow() {
  if (!featureSettings.hideUsageLimitBannerEnabled) {
    restoreUsageLimitOverlays();
    return;
  }
  restoreHiddenComposerHosts();
  for (const overlay of collectUsageLimitOverlays()) {
    hideUsageLimitOverlay(overlay);
  }
}
