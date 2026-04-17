import { AuthService } from "./auth";
import { UserRepository } from "./user";

export const userRepo = new UserRepository();
export const authService = new AuthService(userRepo);

export async function handleRequest(token: string) {
	const result = await authService.authenticate(token);
	return result;
}
