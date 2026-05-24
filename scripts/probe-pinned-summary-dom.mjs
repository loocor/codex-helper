const debugPort = Number(process.env.CODEX_HELPER_DEBUG_PORT || 9230);
const version = await fetch(`http://127.0.0.1:${debugPort}/json/version`).then((r) =>
  r.json(),
);
const page = (await fetch(`http://127.0.0.1:${debugPort}/json/list`).then((r) =>
  r.json(),
)).find((t) => t.type === "page" && /codex/i.test(t.title || ""));

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
  return new Promise((resolve) => {
    const id = ++nextId;
    pending.set(id, resolve);
    ws.send(JSON.stringify({ id, method, params, sessionId }));
  });
}
await new Promise((resolve, reject) => {
  ws.addEventListener("open", resolve, { once: true });
  ws.addEventListener("error", reject, { once: true });
});
const attached = (await call("Target.attachToTarget", {
  targetId: page.id,
  flatten: true,
})).result;
const sessionId = attached.sessionId;

const expression = `(() => {
  function sectionFor(label) {
    return [...document.querySelectorAll("section")].find(
      (section) =>
        section instanceof HTMLElement &&
        (section.textContent || "").includes(label),
    );
  }

  function rowInfo(row) {
    if (!(row instanceof HTMLElement)) return null;
    return {
      className: String(row.className || "").slice(0, 220),
      text: (row.textContent || "").replace(/\\s+/g, " ").trim().slice(0, 80),
      hasAccessory: Boolean(
        row.querySelector("[class*='summary-panel-row-accessory']"),
      ),
      accessoryClass: String(
        row.querySelector("[class*='summary-panel-row-accessory']")?.className ||
          "",
      ).slice(0, 220),
      html: row.outerHTML.slice(0, 1200),
    };
  }

  const environment = sectionFor("Environment");
  const sources = sectionFor("Sources");
  const portForward = sectionFor("Port Forward");

  const envRows = environment
    ? [...environment.querySelectorAll("*")].filter(
        (node) =>
          node instanceof HTMLElement &&
          node.className &&
          String(node.className).includes("summary-panel-row"),
      )
    : [];

  const envList =
    environment?.querySelector("motion.div, div.flex.flex-col.gap-px.px-4") ||
    environment?.querySelector("motion.div, div.relative.z-0.overflow-hidden > div");

  return {
    environment: {
      sectionClass: environment?.className || "",
      listClass: envList?.className || "",
      listHtml: envList?.outerHTML?.slice(0, 900) || "",
      rows: envRows.slice(0, 6).map(rowInfo),
    },
    sources: {
      sectionClass: sources?.className || "",
      row: rowInfo(
        sources?.querySelector("[class*='summary-panel-row']") || null,
      ),
    },
    portForward: {
      sectionClass: portForward?.className || "",
      row: rowInfo(
        portForward?.querySelector("[class*='summary-panel-row']") || null,
      ),
    },
  };
})()`;

const res = await call(
  "Runtime.evaluate",
  { expression, returnByValue: true },
  sessionId,
);
console.log(JSON.stringify(res.result?.result?.value, null, 2));
ws.close();
