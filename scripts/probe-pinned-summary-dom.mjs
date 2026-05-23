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
  const sources = [...document.querySelectorAll("section")].find(
    (section) =>
      section instanceof HTMLElement &&
      (section.textContent || "").includes("Sources") &&
      (section.textContent || "").includes("Web search"),
  );
  const rows = sources
    ? [...sources.querySelectorAll("*")].filter(
        (node) =>
          node instanceof HTMLElement &&
          node.className &&
          String(node.className).includes("summary-panel-row"),
      )
    : [];
  const row = rows.find((node) => (node.textContent || "").includes("Web search"));
  return {
    sectionClass: sources?.className || "",
    headerHtml: sources?.querySelector("header")?.outerHTML?.slice(0, 500) || "",
    listClass: row?.parentElement?.className || "",
    rowHtml: row?.outerHTML?.slice(0, 900) || "",
    rowCount: rows.length,
  };
})()`;

const res = await call(
  "Runtime.evaluate",
  { expression, returnByValue: true },
  sessionId,
);
console.log(JSON.stringify(res.result?.result?.value, null, 2));
ws.close();
