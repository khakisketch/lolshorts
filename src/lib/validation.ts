/**
 * Form validation schemas and utilities
 *
 * Provides validation for YouTube upload and settings forms
 * without requiring the zod dependency.
 */

export interface ValidationError {
  field: string;
  message: string;
}

export interface ValidationResult {
  valid: boolean;
  errors: ValidationError[];
}

// YouTube upload validation
export interface YoutubeUploadInput {
  title: string;
  description?: string;
  tags?: string;
  privacyStatus: "public" | "unlisted" | "private";
}

export function validateYoutubeUpload(
  input: YoutubeUploadInput,
): ValidationResult {
  const errors: ValidationError[] = [];

  if (!input.title || input.title.trim().length === 0) {
    errors.push({ field: "title", message: "Title is required" });
  } else if (input.title.length > 100) {
    errors.push({
      field: "title",
      message: "Title must be 100 characters or less",
    });
  }

  if (input.description && input.description.length > 5000) {
    errors.push({
      field: "description",
      message: "Description must be 5000 characters or less",
    });
  }

  if (input.tags && input.tags.length > 500) {
    errors.push({
      field: "tags",
      message: "Tags must be 500 characters or less",
    });
  }

  const validPrivacyStatuses = ["public", "unlisted", "private"];
  if (!validPrivacyStatuses.includes(input.privacyStatus)) {
    errors.push({
      field: "privacyStatus",
      message: "Privacy status must be public, unlisted, or private",
    });
  }

  return { valid: errors.length === 0, errors };
}

// Settings validation
export interface SettingsInput {
  summoner_name?: string;
}

export function validateSettings(input: SettingsInput): ValidationResult {
  const errors: ValidationError[] = [];

  if (input.summoner_name !== undefined) {
    if (input.summoner_name.trim().length === 0) {
      errors.push({
        field: "summoner_name",
        message: "Summoner name cannot be empty",
      });
    } else if (input.summoner_name.length > 32) {
      errors.push({
        field: "summoner_name",
        message: "Summoner name must be 32 characters or less",
      });
    }
  }

  return { valid: errors.length === 0, errors };
}

/**
 * Returns the first error message for a given field, or undefined
 */
export function getFieldError(
  result: ValidationResult,
  field: string,
): string | undefined {
  return result.errors.find((e) => e.field === field)?.message;
}
