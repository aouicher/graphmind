import { createRequire } from "node:module";
import { Command } from "commander";
import { registerBuildCommand } from "./commands/build.js";
import { registerCleanCommand } from "./commands/clean.js";

const require = createRequire(import.meta.url);
const { version } = require("../../package.json") as { version: string };
import { registerCrossCommand } from "./commands/cross.js";
import { registerDiffImpactCommand } from "./commands/diff-impact.js";
import { registerExcludeCommand } from "./commands/exclude.js";
import { registerExportCommand } from "./commands/export.js";
import { registerHooksCommand } from "./commands/hooks.js";
import { registerMcpCommand } from "./commands/mcp.js";
import { registerMemoryCommand } from "./commands/memory.js";
import { registerQueryCommand } from "./commands/query.js";
import { registerRegisterCommand } from "./commands/register.js";
import { registerSearchCommand } from "./commands/search.js";
import { registerSessionCommand } from "./commands/session.js";
import { registerSyncCommand } from "./commands/sync.js";

const program = new Command();

program.name("graphmind").description("Your codebase has memory. Use it.").version(version);

registerRegisterCommand(program);
registerBuildCommand(program);
registerCleanCommand(program);
registerQueryCommand(program);
registerMemoryCommand(program);
registerCrossCommand(program);
registerSessionCommand(program);
registerDiffImpactCommand(program);
registerExcludeCommand(program);
registerSearchCommand(program);
registerExportCommand(program);
registerHooksCommand(program);
registerSyncCommand(program);
registerMcpCommand(program);

program.parse();
