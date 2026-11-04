import { randomBytes } from "node:crypto";

const SESSION_LIFETIME_MS = 30 * 60 * 1000;
const sessions = new Map<string, { email: string; expiresAt: number }>();

// Creates a session after a successful login and returns its cookie value.
export function createSession(email: string): string {
  const id = randomBytes(16).toString("hex");
  sessions.set(id, { email, expiresAt: Date.now() + SESSION_LIFETIME_MS });
  return id;
}

export function sessionEmail(id: string | undefined): string | undefined {
  if (!id) return undefined;
  const session = sessions.get(id);
  if (!session || session.expiresAt < Date.now()) {
    sessions.delete(id);
    return undefined;
  }
  return session.email;
}
