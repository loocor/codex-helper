import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { dirname } from "node:path";

export type SystemOpenCommand = {
	program: string;
	args: string[];
};

export type SystemOpenOptions = {
	reveal?: boolean;
	preferChrome?: boolean;
};

const GOOGLE_CHROME_APP = "/Applications/Google Chrome.app";

export function systemOpenCommand(
	target: string,
	options: SystemOpenOptions = {},
): SystemOpenCommand {
	switch (process.platform) {
		case "darwin":
			if (options.preferChrome) {
				if (!existsSync(GOOGLE_CHROME_APP)) {
					throw new Error("Google Chrome is required to open DevTools");
				}
				return { program: "open", args: ["-a", "Google Chrome", target] };
			}
			return { program: "open", args: options.reveal ? ["-R", target] : [target] };
		case "win32":
			if (options.reveal) {
				return { program: "explorer.exe", args: ["/select,", target] };
			}
			return { program: "explorer.exe", args: [target] };
		case "linux":
			return {
				program: "xdg-open",
				args: [options.reveal ? dirname(target) : target],
			};
		default:
			throw new Error(`Opening paths is not supported on ${process.platform}`);
	}
}

export function launchSystemOpen(
	target: string,
	options: SystemOpenOptions = {},
): void {
	const command = systemOpenCommand(target, options);
	const child = spawn(command.program, command.args, {
		stdio: "ignore",
		detached: true,
	});
	child.unref();
}
