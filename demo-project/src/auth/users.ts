import { createHash } from "node:crypto";

interface UserRecord {
  email: string;
  passwordHash: string;
}

export function hashPassword(password: string): string {
  return createHash("sha256").update(password).digest("hex");
}

// A tiny in-memory user list. A real app would use a database.
const users: UserRecord[] = [
  { email: "ada@example.com", passwordHash: hashPassword("correct-horse") },
];

export function findUser(email: string): UserRecord | undefined {
  return users.find((u) => u.email === email.toLowerCase());
}

export function passwordMatches(user: UserRecord, password: string): boolean {
  return user.passwordHash === hashPassword(password);
}
