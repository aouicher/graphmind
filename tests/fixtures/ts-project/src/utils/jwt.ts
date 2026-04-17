export interface DecodedToken {
	userId: string;
	exp: number;
}

export function validateToken(token: string): DecodedToken | null {
	if (!token || !token.startsWith("token_")) {
		return null;
	}

	const parts = token.split("_");
	if (parts.length < 3) return null;

	return {
		userId: parts[1]!,
		exp: Number.parseInt(parts[2]!, 10),
	};
}

export function isExpired(decoded: DecodedToken): boolean {
	return decoded.exp < Date.now();
}
