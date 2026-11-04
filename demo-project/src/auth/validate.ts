export interface Credentials {
  email: string;
  password: string;
}

export interface ValidationResult {
  ok: boolean;
  error?: string;
}

const EMAIL_PATTERN = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;
const MIN_PASSWORD_LENGTH = 8;

// Checks the shape of the email and password before we look anyone up.
export function validateCredentials(input: Credentials): ValidationResult {
  if (!EMAIL_PATTERN.test(input.email)) {
    return { ok: false, error: "That does not look like an email address." };
  }
  if (input.password.length < MIN_PASSWORD_LENGTH) {
    return { ok: false, error: `Passwords have at least ${MIN_PASSWORD_LENGTH} characters.` };
  }
  return { ok: true };
}
