import { describe, expect, it } from "vitest";
import { registerGraphTools } from "../src/mcp/tools/graph-tools.js";
import { registerMemoryTools } from "../src/mcp/tools/memory-tools.js";
import { registerMetaTools } from "../src/mcp/tools/meta-tools.js";

describe("MCP Tools Registration", () => {
	it("registers graph tools with correct names", () => {
		const tools = registerGraphTools();
		const names = tools.map((t) => t.name);

		expect(names).toContain("gm_query");
		expect(names).toContain("gm_fn");
		expect(names).toContain("gm_deps");
		expect(names).toContain("gm_impact");
		expect(names).toContain("gm_fn_impact");
		expect(names).toContain("gm_map");
		expect(names).toContain("gm_cycles");
	});

	it("registers memory tools with correct names", () => {
		const tools = registerMemoryTools();
		const names = tools.map((t) => t.name);

		expect(names).toContain("gm_memory_search");
		expect(names).toContain("gm_memory_add");
		expect(names).toContain("gm_memory_list");
	});

	it("registers meta tools with correct names", () => {
		const tools = registerMetaTools();
		const names = tools.map((t) => t.name);

		expect(names).toContain("gm_list_projects");
		expect(names).toContain("gm_status");
		expect(names).toContain("gm_context");
	});

	it("all tools have descriptions and input schemas", () => {
		const allTools = [...registerGraphTools(), ...registerMemoryTools(), ...registerMetaTools()];

		for (const tool of allTools) {
			expect(tool.description).toBeTruthy();
			expect(tool.inputSchema).toBeTruthy();
			expect(typeof tool.handler).toBe("function");
		}
	});

	it("gm_memory_add requires confirmation", async () => {
		const tools = registerMemoryTools();
		const addTool = tools.find((t) => t.name === "gm_memory_add");
		expect(addTool).toBeDefined();

		// biome-ignore lint/style/noNonNullAssertion: guarded by expect above
		const result = await addTool!.handler({ fact: "Test fact", confirmed: false }, null);
		expect(result).toHaveProperty("confirmation_required", true);
		expect(result).toHaveProperty("preview");
	});
});
