const version = await fetch("http://127.0.0.1:9229/json/version").then((r) =>
  r.json(),
);
const page = (await fetch("http://127.0.0.1:9229/json/list").then((r) =>
  r.json(),
)).find((t) => t.type === "page" && t.title === "Codex");

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

const res = await call(
  "Runtime.evaluate",
  {
    expression: `(() => {
      const helper = window.__codexHelperBridge ? 'yes' : 'no';
      const portsCard = document.querySelector('[data-codex-helper-ports-home-card]');
      const panel = document.querySelector('[data-codex-helper-ports-panel]');
      const r = (el) => {
        const b = el.getBoundingClientRect();
        return { tag: el.tagName, top: Math.round(b.top), h: Math.round(b.height) };
      };
      return {
        helper,
        portsCard: portsCard ? r(portsCard) : null,
        panel: panel ? { ...r(panel), parent: r(panel.parentElement) } : null,
        gridFn: typeof findHomeToolCardGrid,
      };
    })()`,
    returnByValue: true,
  },
  sessionId,
);
console.log(JSON.stringify(res.result?.value, null, 2));
ws.close();
