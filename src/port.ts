import { spawnSync } from "node:child_process";

export function isPortFree(port: number): boolean {
	return listenPidsOnPort(port).length === 0;
}

export function listenPidsOnPort(port: number): number[] {
	return listenPidsOnPortWithCommand(port, "lsof");
}

export function listenPidsOnPortWithCommand(
	port: number,
	command: string,
): number[] {
	const result = spawnSync(command, [`-tiTCP:${port}`, "-sTCP:LISTEN"], {
		encoding: "utf8",
	});
	if (result.error) {
		throw new Error(`lsof failed for port ${port}: ${result.error.message}`);
	}
	const stdout = result.stdout.trim();
	const stderr = result.stderr.trim();
	if (result.status !== 0) {
		if (!stdout && !stderr) return [];
		throw new Error(
			`lsof failed for port ${port} with status ${String(result.status)}: ${stderr || stdout}`,
		);
	}
	if (!stdout) return [];
	return result.stdout
		.trim()
		.split("\n")
		.map((value) => Number(value.trim()))
		.filter((pid) => Number.isInteger(pid) && pid > 0);
}

export function processCommand(pid: number): string {
	const result = spawnSync("ps", ["-p", String(pid), "-o", "command="], {
		encoding: "utf8",
	});
	if (result.error) {
		throw new Error(`ps failed for pid ${pid}: ${result.error.message}`);
	}
	if (result.status !== 0) {
		throw new Error(
			`ps failed for pid ${pid} with status ${String(result.status)}: ${result.stderr.trim()}`,
		);
	}
	return result.stdout.trim();
}

export function describePortBlockers(port: number): string {
	return listenPidsOnPort(port)
		.map((pid) => `${pid} (${processCommand(pid)})`)
		.join("; ");
}
