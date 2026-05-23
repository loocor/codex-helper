declare const Bun: {
	argv: string[];
	sleep(ms: number): Promise<void>;
};

declare const process: {
	env: Record<string, string | undefined>;
	platform: string;
	exit(code?: number): never;
	on(event: "SIGINT", listener: () => void): void;
};

declare module "node:fs" {
	export function existsSync(path: string): boolean;
	export function readFileSync(path: string, encoding: "utf8"): string;
	export function writeFileSync(
		path: string,
		data: string,
		encoding: "utf8",
	): void;
	export function appendFileSync(
		path: string,
		data: string,
		encoding: "utf8",
	): void;
	export function readdirSync(path: string): string[];
	export function mkdirSync(
		path: string,
		options: { recursive: boolean },
	): void;
}

declare module "node:os" {
	export function homedir(): string;
}

declare module "node:path" {
	export function join(...parts: string[]): string;
}

declare module "node:child_process" {
	export function spawn(
		command: string,
		args: string[],
		options: {
			stdio: "ignore";
			detached: boolean;
		},
	): {
		unref(): void;
	};
	export function spawnSync(
		command: string,
		args: string[],
		options: { stdio: "ignore" },
	): { status: number | null };
}
