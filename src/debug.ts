export type LaunchStageDetail = Record<
	string,
	string | number | boolean | undefined
>;

export type LaunchTimer = {
	stage: (name: string, detail?: LaunchStageDetail) => void;
};

export function createLaunchTimer(): LaunchTimer {
	const startedAt = Date.now();
	let lastAt = startedAt;
	return {
		stage(name, detail = {}) {
			const now = Date.now();
			const stageMs = now - lastAt;
			const totalMs = now - startedAt;
			lastAt = now;
			const parts = Object.entries(detail)
				.filter(([, value]) => value !== undefined)
				.map(([key, value]) => `${key}=${String(value)}`);
			const suffix = parts.length > 0 ? ` ${parts.join(" ")}` : "";
			console.error(
				`[launch +${totalMs}ms stage +${stageMs}ms] ${name}${suffix}`,
			);
		},
	};
}
