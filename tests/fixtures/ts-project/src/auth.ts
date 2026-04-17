import type { UserRepository } from "./user";
import { validateToken } from "./utils/jwt";

export interface AuthResult {
	valid: boolean;
	userId: string | null;
}

export class AuthService {
	private userRepo: UserRepository;

	constructor(userRepo: UserRepository) {
		this.userRepo = userRepo;
	}

	async authenticate(token: string): Promise<AuthResult> {
		const decoded = validateToken(token);
		if (!decoded) {
			return { valid: false, userId: null };
		}

		const user = await this.userRepo.findById(decoded.userId);
		if (!user) {
			return { valid: false, userId: null };
		}

		return { valid: true, userId: user.id };
	}

	async refreshToken(userId: string): Promise<string> {
		const user = await this.userRepo.findById(userId);
		if (!user) throw new Error("User not found");
		return generateToken(user);
	}
}

function generateToken(user: { id: string; email: string }): string {
	return `token_${user.id}_${Date.now()}`;
}
