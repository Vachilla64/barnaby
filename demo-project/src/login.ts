import { validateCredentials } from "./auth/validate.ts";
import { findUser, passwordMatches } from "./auth/users.ts";
import { createSession } from "./auth/session.ts";

export interface LoginOutcome {
  status: number;
  message: string;
  sessionId?: string;
}

// The login handler: validate the form, find the user, check the password, start a session.
export function handleLogin(body: { email?: string; password?: string }): LoginOutcome {
  const credentials = { email: body.email ?? "", password: body.password ?? "" };

  const check = validateCredentials(credentials);
  if (!check.ok) {
    return { status: 400, message: check.error ?? "Invalid input." };
  }

  const user = findUser(credentials.email);
  if (!user || !passwordMatches(user, credentials.password)) {
    return { status: 401, message: "Wrong email or password." };
  }

  const sessionId = createSession(user.email);
  return { status: 200, message: "Welcome back!", sessionId };
}
