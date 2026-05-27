import { readFileSync } from "node:fs";
import { join } from "node:path";
import { expect, test } from "bun:test";

const packageJson = JSON.parse(
	readFileSync(join(import.meta.dir, "..", "package.json"), "utf8"),
);

test("bun launch builds the Rust bridge binary before starting Codex", () => {
	expect(packageJson.scripts["build:bridge"]).toBe(
		"env RUSTC_WRAPPER= cargo build --manifest-path src-tauri/Cargo.toml --bin codex-helper-bridge",
	);
	expect(packageJson.scripts.launch).toBe(
		"bun run build:bridge && bun src/launch.ts",
	);
});

test("dev launcher configures loopback proxy bypass for both env keys", () => {
	const source = readFileSync(join(import.meta.dir, "launch.ts"), "utf8");

	expect(source).toContain('"127.0.0.1", "localhost", "::1"');
	expect(source).toContain("process.env.NO_PROXY = noProxy");
	expect(source).toContain("process.env.no_proxy = noProxy");
});

test("dev launcher injects all Codex page targets", () => {
	const source = readFileSync(join(import.meta.dir, "launch.ts"), "utf8");

	expect(source).toContain("waitForCodexTargets");
	expect(source).toContain("syncInjectedTargetsForTargets");
	expect(source).toContain("initialSync.failures.length > 0");
	expect(source).toContain("injectedTargets.size !== targets.length");
	expect(source).not.toContain("waitForCodexTarget(");
});

test("dev launcher resolves existing Codex CDP before reserving a launch port", () => {
	const source = readFileSync(join(import.meta.dir, "debug-port.ts"), "utf8");

	expect(source).toContain("findAttachableDebugPort");
	const resolverSource = source.slice(source.indexOf("export async function resolveDebugPort"));
	expect(resolverSource).toContain("await findAttachableDebugPort");
	expect(resolverSource.indexOf("await findAttachableDebugPort")).toBeLessThan(
		resolverSource.indexOf("reserveEphemeralPort()"),
	);
});

test("tauri launcher waits for Codex page targets before injection", () => {
	const controllerSource = readFileSync(
		join(import.meta.dir, "..", "src-tauri", "src", "codex_control.rs"),
		"utf8",
	);
	const cdpSource = readFileSync(
		join(import.meta.dir, "..", "src-tauri", "src", "cdp.rs"),
		"utf8",
	);

	expect(cdpSource).toContain("pub async fn wait_for_codex_targets(");
	expect(controllerSource).toContain("wait_for_codex_targets_ready");
	const launchNewSource = controllerSource.slice(
		controllerSource.indexOf("async fn launch_new_codex"),
		controllerSource.indexOf("pub async fn recover_codex_launch"),
	);
	expect(
		launchNewSource.indexOf(
			'wait_for_codex_targets_ready(&launch_ctx, "initial-launch")',
		),
	).toBeLessThan(
		launchNewSource.indexOf("self.sync_injected_targets(&launch_ctx)"),
	);
	expect(controllerSource).toContain("launcher.codex_targets_ready");
});

test("tauri desktop startup attaches before launching a new Codex", () => {
	const controllerSource = readFileSync(
		join(import.meta.dir, "..", "src-tauri", "src", "codex_control.rs"),
		"utf8",
	);

	expect(controllerSource).toContain("attach_to_existing_codex");
	expect(controllerSource).toContain("launch_new_codex");
	expect(controllerSource).toContain("find_existing_codex_debug_port");
	expect(controllerSource.indexOf("attach_to_existing_codex")).toBeLessThan(
		controllerSource.indexOf("launch_new_codex"),
	);
});

test("tauri startup recovery closes existing CDP before clean launch", () => {
	const controllerSource = readFileSync(
		join(import.meta.dir, "..", "src-tauri", "src", "codex_control.rs"),
		"utf8",
	);
	const cdpSource = readFileSync(
		join(import.meta.dir, "..", "src-tauri", "src", "cdp.rs"),
		"utf8",
	);

	expect(controllerSource).toContain("recover_codex_launch");
	expect(controllerSource).toContain("close_existing_codex_debug_ports");
	expect(cdpSource).toContain("pub async fn wait_for_debug_port_to_close(");
	expect(controllerSource).toContain("close_browser(port).await");
	expect(controllerSource).toContain("wait_for_debug_port_to_close(port");
	const recoverySource = controllerSource.slice(
		controllerSource.indexOf("pub async fn recover_codex_launch"),
		controllerSource.indexOf("async fn close_existing_codex_debug_ports"),
	);
	expect(
		recoverySource.indexOf("close_existing_codex_debug_ports"),
	).toBeLessThan(
		recoverySource.indexOf("launch_new_codex"),
	);
});

test("tauri CDP probing bypasses system proxies", () => {
	const cdpSource = readFileSync(
		join(import.meta.dir, "..", "src-tauri", "src", "cdp.rs"),
		"utf8",
	);
	const appSource = readFileSync(
		join(import.meta.dir, "..", "src-tauri", "src", "app.rs"),
		"utf8",
	);
	const launcherSource = readFileSync(
		join(import.meta.dir, "..", "src-tauri", "src", "launcher.rs"),
		"utf8",
	);
	const appServerSource = readFileSync(
		join(import.meta.dir, "..", "src-tauri", "src", "codex_app_server.rs"),
		"utf8",
	);

	expect(cdpSource).toContain("fn cdp_http_client()");
	expect(cdpSource).toContain(".no_proxy()");
	expect(appSource).toContain("configure_process_loopback_no_proxy();");
	expect(launcherSource).toContain('env("NO_PROXY"');
	expect(launcherSource).toContain('env("no_proxy"');
	expect(launcherSource).toContain('"--remote-debugging-address=127.0.0.1"');
	expect(launcherSource).not.toContain("quit_existing_codex_processes");
	expect(appServerSource).toContain('env("NO_PROXY"');
	expect(appServerSource).toContain('env("no_proxy"');
});

test("tauri target watcher backs off after failures", () => {
	const controllerSource = readFileSync(
		join(import.meta.dir, "..", "src-tauri", "src", "codex_control.rs"),
		"utf8",
	);

	expect(controllerSource).toContain("TARGET_WATCHER_MAX_RECONNECT");
	expect(controllerSource).toContain("next_target_watcher_reconnect_delay");
	expect(controllerSource).toContain("checked_mul(2)");
});

test("tauri target watcher coalesces discovery events before resync", () => {
	const controllerSource = readFileSync(
		join(import.meta.dir, "..", "src-tauri", "src", "codex_control.rs"),
		"utf8",
	);

	expect(controllerSource).toContain("TARGET_EVENT_DEBOUNCE");
	expect(controllerSource).toContain("drain_target_discovery_events");
	expect(controllerSource).toContain("TARGET_WATCHER_DISCONNECT_PROBE_LIMIT");
	expect(controllerSource).toContain("target_watcher_disconnect_probe");
});

test("dev launcher keeps syncing Codex page target changes", () => {
	const source = readFileSync(join(import.meta.dir, "launch.ts"), "utf8");
	const syncSource = readFileSync(
		join(import.meta.dir, "injection-sync.ts"),
		"utf8",
	);

	expect(source).toContain("startCodexTargetWatcher({");
	expect(syncSource).toContain("Target.setDiscoverTargets");
	expect(syncSource).toContain("ALL_TARGETS_FILTER");
	expect(syncSource).toContain("Target.targetCreated");
	expect(syncSource).toContain("Target.targetInfoChanged");
	expect(syncSource).toContain("Target.targetDestroyed");
	expect(source).toContain("injectedTargets.clear()");
	expect(source).not.toContain("Bun.sleep(2000)");
});

test("dev launcher keeps macOS app launch behind the launch adapter", () => {
	const source = readFileSync(join(import.meta.dir, "launcher.ts"), "utf8");

	expect(source).not.toContain('"osascript"');
	expect(source).toContain('program: "open"');
	expect(source).toContain('"-na"');
	expect(source).not.toContain("quit codex start");
	expect(source).not.toContain("pgrep");
});

test("dev bridge keeps platform open commands behind an adapter", () => {
	const routesSource = readFileSync(join(import.meta.dir, "routes.ts"), "utf8");
	const zedSource = readFileSync(join(import.meta.dir, "zed.ts"), "utf8");

	expect(routesSource).toContain("launchSystemOpen");
	expect(routesSource).not.toContain('spawn("open"');
	expect(zedSource).not.toContain('spawn("open"');
	const launchSource = zedSource.slice(
		zedSource.indexOf("function launchZedUrl"),
	);
	expect(launchSource.indexOf("if (cliPath)")).toBeLessThan(
		launchSource.indexOf('process.platform === "darwin"'),
	);
});

test("tauri routes use opener APIs instead of spawning macOS open", () => {
	const source = readFileSync(
		join(import.meta.dir, "..", "src-tauri", "src", "routes.rs"),
		"utf8",
	);
	const zedSource = readFileSync(
		join(import.meta.dir, "..", "src-tauri", "src", "zed.rs"),
		"utf8",
	);

	expect(source).toContain("tauri_plugin_opener");
	expect(source).not.toContain('Command::new("open")');
	expect(source).not.toContain('std::process::Command::new("open")');
	expect(zedSource).toContain("tauri_plugin_opener::open_url");
	expect(zedSource).not.toContain('Command::new("open")');
});

test("tauri build script generates tray icons without sips", () => {
	const source = readFileSync(
		join(import.meta.dir, "..", "src-tauri", "build.rs"),
		"utf8",
	);

	expect(source).not.toContain('"sips"');
	expect(source).not.toContain("std::process::Command");
});
