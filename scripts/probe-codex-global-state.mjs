const debugPort = Number(process.env.CODEX_HELPER_DEBUG_PORT || 9230);
const version = await fetch(`http://127.0.0.1:${debugPort}/json/version`).then((r) =>
  r.json(),
);
const page = (await fetch(`http://127.0.0.1:${debugPort}/json/list`).then((r) =>
  r.json(),
)).find((t) => t.type === "page" && /codex/i.test(t.title || ""));

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
  const attrs = new Set();
  for (const el of document.querySelectorAll("*")) {
    if (!(el instanceof HTMLElement)) continue;
    for (const name of el.getAttributeNames()) {
      if (/terminal|port|pty|shell|forward|tunnel|host|thread|session/i.test(name)) {
        attrs.add(name);
      }
    }
  }

  const hostIds = [...document.querySelectorAll("[data-app-action-sidebar-thread-host-id]")]
    .slice(0, 8)
    .map((el) => ({
      value: el.getAttribute("data-app-action-sidebar-thread-host-id"),
      selected: el.getAttribute("aria-selected"),
      state: el.getAttribute("data-state"),
      visible: (() => {
        const b = el.getBoundingClientRect();
        return b.width > 0 && b.height > 0;
      })(),
    }));

  const windowKeys = Object.keys(window).filter((k) =>
    /terminal|port|pty|shell|codex|thread|session|forward/i.test(k),
  );

  const reactKeys = [];
  for (const key of Object.keys(window)) {
    if (!/react|fiber|store|state|__/.test(key)) continue;
    try {
      const v = window[key];
      if (v && typeof v === "object") reactKeys.push(key);
    } catch {}
  }

  const xtermCount = document.querySelectorAll(".xterm").length;
  const pinned = document.querySelector("[data-codex-helper-ports-pinned]");

  return {
    title: document.title,
    xtermCount,
    pinnedConnected: Boolean(pinned?.isConnected),
    hostIds,
    dataAttributes: [...attrs].sort().slice(0, 80),
    windowKeys: windowKeys.sort().slice(0, 60),
    reactLikeKeys: reactKeys.sort().slice(0, 40),
  };
})()`;

const res = await call(
  "Runtime.evaluate",
  { expression, returnByValue: true },
  sessionId,
);
if (res.result?.exceptionDetails) {
  console.error(JSON.stringify(res.result.exceptionDetails, null, 2));
  process.exit(1);
}
console.log(JSON.stringify(res.result?.result?.value ?? res.result?.value, null, 2));
ws.close();
