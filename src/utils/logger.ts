import chalk from "chalk";

const out = (...args: unknown[]) => console.error(...args);

export const log = {
	info: (msg: string) => out(chalk.blue("ℹ"), msg),
	success: (msg: string) => out(chalk.green("✓"), msg),
	warn: (msg: string) => out(chalk.yellow("⚠"), msg),
	error: (msg: string) => out(chalk.red("✗"), msg),
	dim: (msg: string) => out(chalk.dim(msg)),
};
