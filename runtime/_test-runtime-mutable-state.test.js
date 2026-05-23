import { expect, test } from "bun:test";
import { buildRuntimeBundle } from "./bundle.ts";

test("bundled runtime keeps mutable featureSettings state", () => {
	const source = buildRuntimeBundle();
	expect(source).toContain("let featureSettings = {");
	expect(source).not.toContain("const featureSettings = {");
});
