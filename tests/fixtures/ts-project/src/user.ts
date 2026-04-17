export interface User {
	id: string;
	email: string;
	name: string;
}

export class UserRepository {
	private users: Map<string, User> = new Map();

	async findById(id: string): Promise<User | undefined> {
		return this.users.get(id);
	}

	async create(user: User): Promise<User> {
		this.users.set(user.id, user);
		return user;
	}

	async delete(id: string): Promise<boolean> {
		return this.users.delete(id);
	}
}

export function formatUserName(user: User): string {
	return `${user.name} <${user.email}>`;
}
