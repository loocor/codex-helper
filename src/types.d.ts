declare const Bun: {
  argv: string[];
  sleep(ms: number): Promise<void>;
};

declare const process: {
  exit(code?: number): never;
};

declare module "node:fs" {
  export function existsSync(path: string): boolean;
  export function readFileSync(path: string, encoding: "utf8"): string;
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
}
