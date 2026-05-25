import { writeFileSync } from "node:fs";

const debugPort = Number(process.env.CODEX_HELPER_DEBUG_PORT || 9229);
const screenshotPath =
  process.env.CODEX_HELPER_VALIDATE_SCREENSHOT ||
  "/private/tmp/codex-helper-native-settings-logs.png";

const version = await fetch(`http://127.0.0.1:${debugPort}/json/version`).then(
  (response) => response.json(),
);
const page = (await fetch(`http://127.0.0.1:${debugPort}/json/list`).then(
  (response) => response.json(),
)).find((target) => target.type === "page" && /codex/i.test(target.title || ""));

if (!page) {
  console.error("No Codex page target found");
  process.exit(1);
}

const ws = new WebSocket(version.webSocketDebuggerUrl);
let nextId = 0;
const pending = new Map();

ws.addEventListener("message", (event) => {
  const message = JSON.parse(String(event.data));
  if (message.id && pending.has(message.id)) {
    pending.get(message.id)(message);
    pending.delete(message.id);
  }
});

function call(method, params = {}, sessionId) {
  return new Promise((resolve, reject) => {
    const id = ++nextId;
    const timeout = setTimeout(() => {
      pending.delete(id);
      reject(new Error(`CDP call timed out: ${method}`));
    }, 10000);
    pending.set(id, (message) => {
      clearTimeout(timeout);
      resolve(message);
    });
    ws.send(JSON.stringify({ id, method, params, sessionId }));
  });
}

await new Promise((resolve, reject) => {
  ws.addEventListener("open", resolve, { once: true });
  ws.addEventListener("error", reject, { once: true });
});

const attached = (
  await call("Target.attachToTarget", {
    targetId: page.id,
    flatten: true,
  })
).result;
const sessionId = attached.sessionId;
await call("Page.enable", {}, sessionId);
await call("Runtime.enable", {}, sessionId);

async function evaluate(expression) {
  const response = await call(
    "Runtime.evaluate",
    { expression, returnByValue: true },
    sessionId,
  );
  if (response.result?.exceptionDetails) {
    throw new Error(JSON.stringify(response.result.exceptionDetails, null, 2));
  }
  return response.result?.result?.value;
}

async function clickPoint(point) {
  await call(
    "Input.dispatchMouseEvent",
    { type: "mouseMoved", x: point.x, y: point.y },
    sessionId,
  );
  await call(
    "Input.dispatchMouseEvent",
    {
      type: "mousePressed",
      x: point.x,
      y: point.y,
      button: "left",
      clickCount: 1,
    },
    sessionId,
  );
  await call(
    "Input.dispatchMouseEvent",
    {
      type: "mouseReleased",
      x: point.x,
      y: point.y,
      button: "left",
      clickCount: 1,
    },
    sessionId,
  );
}

async function delay(ms) {
  await new Promise((resolve) => setTimeout(resolve, ms));
}

async function settingsState() {
  return evaluate(`(() => {
    const textOf = (node) => (node?.textContent || "").replace(/\\s+/g, " ").trim();
    const sidebar = Array.from(document.querySelectorAll("aside, nav, [role='navigation'], [role='tablist']"))
      .find((node) => {
        const text = textOf(node);
        return text.includes("Back to app") &&
          text.includes("General") &&
          text.includes("Appearance") &&
          text.includes("Configuration") &&
          text.includes("Personalization");
      });
    const group = sidebar?.querySelector("[data-codex-helper-native-settings-group]");
    const entries = group
      ? Array.from(group.querySelectorAll("[data-codex-helper-native-settings-entry]")).map((entry) => ({
        page: entry.getAttribute("data-codex-helper-native-settings-entry"),
        text: textOf(entry),
        active: entry.getAttribute("data-active"),
        hasIcon: Boolean(entry.querySelector(".codex-helper-native-settings-sidebar-icon")),
        iconName: entry.querySelector(".codex-helper-native-settings-sidebar-icon")?.getAttribute("data-lucide") || "",
      }))
      : [];
    const host = sidebar
      ? Array.from(sidebar.querySelectorAll("*")).find((node) => textOf(node) === "Host")
      : null;
    const helperPage = document.querySelector("[data-codex-helper-native-settings-page]");
    const helperSurface = helperPage
      ? Array.from(document.querySelectorAll(".main-surface")).find((node) => node.contains(helperPage))
      : null;
    const helperPanel = helperPage?.querySelector(".codex-helper-panel");
    const helperTitle = helperPage?.querySelector(".heading-base");
    const pathHeader = helperPage?.querySelector(".codex-helper-native-settings-list-header");
    const listFooter = helperPage?.querySelector(".codex-helper-native-settings-list-footer");
    const iconButtons = helperPage
      ? helperPage.querySelectorAll(".codex-helper-native-settings-icon-button").length
      : 0;
    const surfaceStyle = helperSurface ? getComputedStyle(helperSurface) : null;
    const panelStyle = helperPanel ? getComputedStyle(helperPanel) : null;
    const groupRect = group?.getBoundingClientRect();
    const hostRect = host?.getBoundingClientRect();
    return {
      href: location.href,
      sidebarFound: Boolean(sidebar),
      groupFound: Boolean(group),
      groupAfterHost: Boolean(groupRect && hostRect && groupRect.top > hostRect.top),
      entries,
      helperPageCount: document.querySelectorAll("[data-codex-helper-native-settings-page]").length,
      activePage: helperPage?.getAttribute("data-codex-helper-native-settings-page") || "",
      helperSurfaceBackground: surfaceStyle?.backgroundColor || "",
      helperSurfaceRadius: surfaceStyle?.borderTopLeftRadius || "",
      helperPanelBorder: panelStyle?.borderTopColor || "",
      helperTitleClass: helperTitle?.className || "",
      pathHeaderFound: Boolean(pathHeader),
      listFooterFound: Boolean(listFooter),
      iconButtons,
      text: textOf(document.body).slice(0, 900),
    };
  })()`);
}

async function pointFor(expression) {
  return evaluate(`(() => {
    const node = (${expression})();
    if (!node) return null;
    const rect = node.getBoundingClientRect();
    if (rect.width <= 0 || rect.height <= 0) return null;
    return { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 };
  })()`);
}

async function openNativeSettingsIfNeeded() {
  if ((await settingsState()).sidebarFound) return;
  const trigger = await pointFor(`() => {
    const textOf = (node) => (node?.textContent || "").replace(/\\s+/g, " ").trim();
    return Array.from(document.querySelectorAll("button")).find((node) => textOf(node) === "Settings");
  }`);
  if (!trigger) throw new Error("Settings menu trigger not found");
  await clickPoint(trigger);
  await delay(400);
  const menuItem = await pointFor(`() => {
    const textOf = (node) => (node?.textContent || "").replace(/\\s+/g, " ").trim();
    return Array.from(document.querySelectorAll('[role="menuitem"]')).find((node) => textOf(node) === "Settings");
  }`);
  if (!menuItem) throw new Error("Native Settings menu item not found");
  await clickPoint(menuItem);
  await delay(1200);
}

async function clickHelperEntry(pageId) {
  return evaluate(`(() => {
    const entry = document.querySelector('[data-codex-helper-native-settings-entry="${pageId}"]');
    if (!entry) return { ok: false, reason: "entry not found" };
    entry.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, view: window }));
    return {
      ok: true,
      activePage: document.querySelector("[data-codex-helper-native-settings-page]")?.getAttribute("data-codex-helper-native-settings-page") || "",
      helperPageCount: document.querySelectorAll("[data-codex-helper-native-settings-page]").length,
    };
  })()`);
}

async function clickNativeGeneral() {
  return evaluate(`(() => {
    const textOf = (node) => (node?.textContent || "").replace(/\\s+/g, " ").trim();
    const entry = Array.from(document.querySelectorAll("button, a, [role='button'], [role='tab'], [role='menuitem']"))
      .find((node) => textOf(node) === "General" && !node.closest("[data-codex-helper-native-settings-group]"));
    if (!entry) return { ok: false, reason: "native General not found" };
    entry.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, view: window }));
    return {
      ok: document.querySelectorAll("[data-codex-helper-native-settings-page]").length === 0,
      helperPageCount: document.querySelectorAll("[data-codex-helper-native-settings-page]").length,
    };
  })()`);
}

const failures = [];
await openNativeSettingsIfNeeded();
let state = await settingsState();
const labels = state.entries.map((entry) => entry.text).join("|");
if (!state.sidebarFound) failures.push("settings sidebar not found");
if (!state.groupFound) failures.push("Helper group not inserted");
if (!state.groupAfterHost) failures.push("Helper group is not positioned after Host");
if (labels !== "General|Deleted Sessions|Logs|About") {
  failures.push(`unexpected Helper labels: ${labels}`);
}
if (state.entries.some((entry) => !entry.hasIcon)) {
  failures.push("Helper sidebar entries are missing contextual icons");
}
const iconNames = state.entries.map((entry) => entry.iconName).join("|");
if (iconNames !== "sliders-horizontal|trash-2|scroll-text|info") {
  failures.push(`unexpected Helper icon names: ${iconNames}`);
}

for (let cycle = 0; cycle < 6; cycle += 1) {
  for (const pageId of ["general", "deleted-sessions", "logs", "about"]) {
    const result = await clickHelperEntry(pageId);
    if (!result.ok) failures.push(`cycle ${cycle} ${pageId}: ${result.reason}`);
    if (result.activePage !== pageId) {
      failures.push(`cycle ${cycle} expected ${pageId}, saw ${result.activePage}`);
    }
    if (result.helperPageCount !== 1) {
      failures.push(`cycle ${cycle} expected one Helper page, saw ${result.helperPageCount}`);
    }
  }
  const cleanup = await clickNativeGeneral();
  if (!cleanup.ok) {
    failures.push(`cycle ${cycle} native cleanup failed: ${cleanup.reason || cleanup.helperPageCount}`);
  }
}

await clickHelperEntry("about");
await delay(300);
state = await settingsState();
if (!state.text.includes("Codex Helper")) {
  failures.push("About page is missing the project name");
}
if (!state.text.includes("Last updated")) {
  failures.push("About page is missing the update date row");
}
if (!state.text.includes("github.com/loocor/codex-helper")) {
  failures.push("About page is missing the repository link");
}

await clickHelperEntry("logs");
await delay(500);
state = await settingsState();
if (!state.pathHeaderFound) {
  failures.push("Logs page is missing the native settings path header");
}
if (!state.listFooterFound) {
  failures.push("Logs page is missing the native settings list footer");
}
if (state.iconButtons < 2) {
  failures.push(`Logs page expected refresh/open icon buttons, saw ${state.iconButtons}`);
}
if (state.helperSurfaceBackground !== "rgb(255, 255, 255)") {
  failures.push(`unexpected Helper surface background: ${state.helperSurfaceBackground}`);
}
if (Number.parseFloat(state.helperSurfaceRadius || "0") <= 0) {
  failures.push(`Helper surface has no rounded corner: ${state.helperSurfaceRadius}`);
}
if (
  !state.helperPanelBorder ||
  state.helperPanelBorder === "rgba(0, 0, 0, 0)" ||
  state.helperPanelBorder === "transparent"
) {
  failures.push(`Helper panel border is not visible: ${state.helperPanelBorder}`);
}
if (!String(state.helperTitleClass).includes("heading-base")) {
  failures.push(`Helper page title does not use heading-base: ${state.helperTitleClass}`);
}

const screenshot = await call(
  "Page.captureScreenshot",
  { format: "png", captureBeyondViewport: false },
  sessionId,
);
writeFileSync(screenshotPath, Buffer.from(screenshot.result.data, "base64"));

const result = {
  ok: failures.length === 0,
  failures,
  activePage: state.activePage,
  helperPageCount: state.helperPageCount,
  entries: state.entries,
  screenshotPath,
};
console.log(JSON.stringify(result, null, 2));
ws.close();

if (!result.ok) process.exit(1);
