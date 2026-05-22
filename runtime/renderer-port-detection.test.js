import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";

const source = readFileSync(new URL("./renderer.js", import.meta.url), "utf8");

function extractFunction(name) {
  const marker = `function ${name}(`;
  const start = source.indexOf(marker);
  if (start === -1) throw new Error(`${name} not found`);
  const braceStart = source.indexOf("{", start);
  let depth = 0;
  for (let index = braceStart; index < source.length; index += 1) {
    const char = source[index];
    if (char === "{") depth += 1;
    if (char === "}") depth -= 1;
    if (depth === 0) return source.slice(start, index + 1);
  }
  throw new Error(`${name} closing brace not found`);
}

function loadFunction(name) {
  return new Function(`${extractFunction(name)}; return ${name};`)();
}

test("parseWebPortsFromText extracts clear web URLs", () => {
  const parseWebPortsFromText = loadFunction("parseWebPortsFromText");

  expect(
    parseWebPortsFromText(`
      Vite ready at http://localhost:5173/
      API listening on http://127.0.0.1:8000/health
      Preview: http://0.0.0.0:3000
      IPv6: http://[::1]:7000/path
    `),
  ).toEqual([
    { port: 5173, url: "http://localhost:5173/" },
    { port: 8000, url: "http://127.0.0.1:8000/health" },
    { port: 3000, url: "http://0.0.0.0:3000" },
    { port: 7000, url: "http://[::1]:7000/path" },
  ]);
});

test("parseWebPortsFromText ignores ambiguous and invalid ports", () => {
  const parseWebPortsFromText = loadFunction("parseWebPortsFromText");

  expect(
    parseWebPortsFromText(`
      listening on port 5432
      invalid http://localhost:70000
      not local http://example.com:3000
    `),
  ).toEqual([]);
});

test("terminal port scanner does not read helper-owned panel text directly", () => {
  expect(source).toContain("function terminalTextForPortScan()");
  expect(source).toContain("const text = terminalTextForPortScan();");
  expect(source).not.toContain("const text = textOf(document.body);");
});

test("terminal port scanner is scoped to terminal-like roots", () => {
  const terminalTextForPortScan = extractFunction("terminalTextForPortScan");

  expect(source).toContain("function findTerminalPortScanRoots()");
  expect(terminalTextForPortScan).not.toContain("createTreeWalker(document.body");
  expect(terminalTextForPortScan).toContain("findTerminalPortScanRoots()");
});

test("detected ports require local port choice when same-port default is disabled", () => {
  const localPortForDetectedPort = new Function(
    "featureSettings",
    `${extractFunction("localPortForDetectedPort")}; return localPortForDetectedPort;`,
  )({ portSameLocalPort: false });
  const shouldAutoForwardDetectedPort = new Function(
    "featureSettings",
    `${extractFunction("shouldAutoForwardDetectedPort")}; return shouldAutoForwardDetectedPort;`,
  )({ portAutoForwardWeb: true });

  expect(localPortForDetectedPort(5173)).toBe(0);
  expect(shouldAutoForwardDetectedPort({ localPort: 0 }, { hostId: "remote" })).toBe(false);
});

test("port keys keep custom local port choices distinct", () => {
  const portKey = loadFunction("portKey");
  const context = { hostId: "remote", path: "/srv/app" };

  expect(portKey(context, 5173, 0)).toBe("remote:/srv/app:5173:custom");
  expect(portKey(context, 5173, 15173)).toBe("remote:/srv/app:5173:15173");
});
