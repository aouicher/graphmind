import { Command } from "commander";
import { registerBuildCommand } from "./commands/build.js";
import { registerCrossCommand } from "./commands/cross.js";
import { registerDiffImpactCommand } from "./commands/diff-impact.js";
import { registerMcpCommand } from "./commands/mcp.js";
import { registerMemoryCommand } from "./commands/memory.js";
import { registerQueryCommand } from "./commands/query.js";
import { registerRegisterCommand } from "./commands/register.js";
import { registerSessionCommand } from "./commands/session.js";
import { registerSyncCommand } from "./commands/sync.js";

const program = new Command();

program
	.name("graphmind")
	.description("Your codebase has memory. Use it.")
	.version("0.1.0");

registerRegisterCommand(program);
registerBuildCommand(program);
registerQueryCommand(program);
registerMemoryCommand(program);
registerCrossCommand(program);
registerSessionCommand(program);
registerDiffImpactCommand(program);
registerSyncCommand(program);
registerMcpCommand(program);

program.parse();
