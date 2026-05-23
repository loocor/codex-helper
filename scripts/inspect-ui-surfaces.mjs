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

const expression = `(() => {
  function sig(el) {
    const b = el.getBoundingClientRect();
    return {
      tag: el.tagName,
      top: Math.round(b.top),
      left: Math.round(b.left),
      h: Math.round(b.height),
      w: Math.round(b.width),
      role: el.getAttribute("role") || "",
      testid: el.getAttribute("data-testid") || "",
      panel: el.getAttribute("data-panel-id") || "",
      text: (el.textContent || "").replace(/\\s+/g, " ").trim().slice(0, 70),
    };
  }

  const needles = [
    "Pinned Summary",
    "Environment",
    "Sources",
    "Forward remote ports",
    "Files",
    "bun run",
  ];
  const hits = {};
  for (const needle of needles) {
    hits[needle] = [];
    for (const el of document.querySelectorAll("*")) {
      if (!(el instanceof HTMLElement)) continue;
      const text = (el.textContent || "").replace(/\\s+/g, " ").trim();
      if (!text.includes(needle)) continue;
      const b = el.getBoundingClientRect();
      if (b.width < 40 || b.height < 8) continue;
      hits[needle].push(sig(el));
      if (hits[needle].length >= 5) break;
    }
  }

  const portsPanel = document.querySelector("[data-codex-helper-ports-panel]");
  const portsCard = document.querySelector("[data-codex-helper-ports-home-card]");

  return {
    ih: innerHeight,
    portsPanel: portsPanel ? { ...sig(portsPanel), connected: portsPanel.isConnected } : null,
    portsCard: portsCard ? sig(portsCard) : null,
    hits,
  };
})()`;

const res = await call(
  "Runtime.evaluate",
  { expression, returnByValue: true },
  sessionId,
);
console.log(JSON.stringify(res.result?.value, null, 2));
ws.close();
