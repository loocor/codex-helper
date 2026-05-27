import { spawn } from "node:child_process";
import { dirname } from "node:path";

export type SystemOpenCommand = {
	program: string;
	args: string[];
};

export function systemOpenCommand(
	target: string,
	options: { reveal?: boolean } = {},
): SystemOpenCommand {
	switch (process.platform) {
		case "darwin":
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
	options: { reveal?: boolean } = {},
): void {
	const command = systemOpenCommand(target, options);
	const child = spawn(command.program, command.args, {
		stdio: "ignore",
		detached: true,
	});
	child.unref();
}
