// biome-ignore lint/suspicious/noExplicitAny: @xenova/transformers has no exported pipeline type
let pipeline: any = null;
let loadAttempted = false;

// biome-ignore lint/suspicious/noExplicitAny: dynamic import with no type exports
async function loadPipeline(): Promise<any> {
	if (pipeline) return pipeline;
	if (loadAttempted) return null;
	loadAttempted = true;

	try {
		const { pipeline: createPipeline } = await import("@xenova/transformers");
		pipeline = await createPipeline("feature-extraction", "Xenova/all-MiniLM-L6-v2", {
			quantized: true,
		});
		return pipeline;
	} catch {
		return null;
	}
}

export async function embed(text: string): Promise<Float32Array | null> {
	const pipe = await loadPipeline();
	if (!pipe) return null;

	const output = await pipe(text, { pooling: "mean", normalize: true });
	return output.data as Float32Array;
}

export async function embedBatch(texts: string[]): Promise<Array<Float32Array | null>> {
	const pipe = await loadPipeline();
	if (!pipe) return texts.map(() => null);

	const results: Array<Float32Array | null> = [];
	for (const text of texts) {
		const output = await pipe(text, { pooling: "mean", normalize: true });
		results.push(output.data as Float32Array);
	}
	return results;
}

export async function isAvailable(): Promise<boolean> {
	const pipe = await loadPipeline();
	return pipe !== null;
}
