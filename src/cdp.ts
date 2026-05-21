export type CdpTarget = {
  id: string;
  type: string;
  title?: string;
  url?: string;
  webSocketDebuggerUrl?: string;
};

export async function listTargets(debugPort: number): Promise<CdpTarget[]> {
  const response = await fetch(`http://127.0.0.1:${debugPort}/json`);
  if (!response.ok) {
    throw new Error(`CDP target query failed: ${response.status} ${response.statusText}`);
  }
  return response.json() as Promise<CdpTarget[]>;
}

export function pickCodexPageTarget(targets: CdpTarget[]): CdpTarget {
  const pages = targets.filter((target) => target.type === "page" && target.webSocketDebuggerUrl);
  const codexPage = pages.find((target) => `${target.title ?? ""} ${target.url ?? ""}`.toLowerCase().includes("codex"));
  const selected = codexPage ?? pages[0];
  if (!selected?.webSocketDebuggerUrl) {
    throw new Error("No injectable Codex page target found");
  }
  return selected;
}

export async function waitForCodexTarget(debugPort: number, attempts = 40): Promise<CdpTarget> {
  let lastError: unknown;
  for (let attempt = 0; attempt < attempts; attempt += 1) {
    try {
      return pickCodexPageTarget(await listTargets(debugPort));
    } catch (error) {
      lastError = error;
      await Bun.sleep(250);
    }
  }
  throw new Error(`Timed out waiting for Codex CDP target: ${String(lastError)}`);
}

export async function cdpCommand(webSocketUrl: string, method: string, params: unknown): Promise<unknown> {
  const socket = new WebSocket(webSocketUrl);
  await new Promise<void>((resolve, reject) => {
    socket.addEventListener("open", () => resolve(), { once: true });
    socket.addEventListener("error", () => reject(new Error(`Failed to connect CDP websocket: ${webSocketUrl}`)), { once: true });
  });

  const id = 1;
  const result = await new Promise<unknown>((resolve, reject) => {
    socket.addEventListener("message", (event) => {
      const message = JSON.parse(String(event.data)) as { id?: number; error?: unknown; result?: unknown };
      if (message.id !== id) return;
      if (message.error) reject(new Error(`CDP command ${method} failed: ${JSON.stringify(message.error)}`));
      else resolve(message.result);
    });
    socket.send(JSON.stringify({ id, method, params }));
  });
  socket.close();
  return result;
}
