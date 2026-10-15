// Personal Notepad — Password Validation Helpers (Phase 12, Task 7)

export const MIN_PASSWORD_LENGTH = 8;
export const MAX_PASSWORD_LENGTH = 1024;

export interface PasswordValidationResult {
  isValid: boolean;
  errorMessage?: string;
}

/**
 * Validates password constraints without introducing arbitrary complexity rules.
 * Aligns strictly with Rust backend bounds: min 8 chars, max 1024 chars, non-empty.
 */
export function validatePassword(password: string): PasswordValidationResult {
  if (!password || password.trim().length === 0) {
    return {
      isValid: false,
      errorMessage: "Password cannot be empty or only whitespace.",
    };
  }

  if (password.length < MIN_PASSWORD_LENGTH) {
    return {
      isValid: false,
      errorMessage: `Password must be at least ${MIN_PASSWORD_LENGTH} characters long.`,
    };
  }

  if (password.length > MAX_PASSWORD_LENGTH) {
    return {
      isValid: false,
      errorMessage: `Password exceeds maximum limit of ${MAX_PASSWORD_LENGTH} characters.`,
    };
  }

  return { isValid: true };
}

/**
 * Validates that primary and confirmation passwords match exactly.
 */
export function validatePasswordMatch(password: string, confirm: string): PasswordValidationResult {
  const basic = validatePassword(password);
  if (!basic.isValid) {
    return basic;
  }

  if (password !== confirm) {
    return {
      isValid: false,
      errorMessage: "Passwords do not match. Please check both fields.",
    };
  }

  return { isValid: true };
}

/**
 * Validates password change inputs:
 * 1. Current password cannot be empty.
 * 2. New password must satisfy length constraints.
 * 3. New password must be different from current password.
 * 4. Confirmation must match new password exactly.
 */
export function validateChangePassword(
  currentPassword: string,
  newPassword: string,
  confirmNewPassword: string
): PasswordValidationResult {
  if (!currentPassword || currentPassword.trim().length === 0) {
    return {
      isValid: false,
      errorMessage: "Please enter your current encryption password.",
    };
  }

  const newValidation = validatePassword(newPassword);
  if (!newValidation.isValid) {
    return newValidation;
  }

  if (currentPassword === newPassword) {
    return {
      isValid: false,
      errorMessage: "New password must be different from your current password.",
    };
  }

  if (newPassword !== confirmNewPassword) {
    return {
      isValid: false,
      errorMessage: "New password confirmation does not match.",
    };
  }

  return { isValid: true };
}

