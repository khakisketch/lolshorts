/**
 * IPC argument-key conversion for Tauri commands.
 *
 * Tauri v2 derives a command's argument keys from the Rust parameter names using
 * heck's `to_lower_camel_case` (tauri-macros' default `ArgumentCase::Camel`), and
 * resolves them with an exact `v.get(key)` — there is no snake_case fallback. This
 * codebase writes snake_case on the frontend, so the keys are converted here on the
 * way out.
 *
 * This lives in its own module, free of `import.meta`, so it can be unit-tested
 * directly. `client.ts` cannot: `jest.setup.js` mocks it wholesale for every suite
 * to dodge `import.meta.env`, which is precisely why this boundary went uncovered.
 */

/**
 * Converts a single snake_case (or already lowerCamelCase) object key to
 * lowerCamelCase.
 *
 * Rule: split on '_', drop empty pieces (so leading underscores like
 * `_force_refresh` behave like heck), keep the first piece as-is, and upper-case
 * the first letter of every subsequent piece. Idempotent on keys that are already
 * lowerCamelCase (no underscores -> single piece -> returned unchanged).
 */
export function snakeKeyToCamel(key: string): string {
  const parts = key.split("_").filter((part) => part.length > 0);
  if (parts.length === 0) {
    return key;
  }
  const [first, ...rest] = parts;
  return (
    first +
    rest.map((part) => part.charAt(0).toUpperCase() + part.slice(1)).join("")
  );
}

/**
 * Whether a key is in the shape this converter handles faithfully.
 *
 * Rust parameter names are lowercase snake_case, which is the subset where
 * `snakeKeyToCamel` is exactly equivalent to heck. Outside it the two diverge
 * (heck lowercases the remainder of each word, so `videoID` -> `videoId` while
 * this function leaves `videoID` alone) and the resulting key would simply not
 * exist on the Rust side — Tauri answers with an opaque "missing required key".
 * Rather than reimplement heck's acronym rules for inputs that never occur, the
 * unsupported shape is reported so it fails loudly in development.
 */
export function isSupportedArgKey(key: string): boolean {
  // Pure lowercase snake_case (leading/repeated underscores included): converting it
  // is exactly what heck does.
  if (/^_*[a-z][a-z0-9_]*$/.test(key)) {
    return true;
  }
  // No underscores at all: passed through unchanged, so it is whatever the author
  // typed. A wrong one here is an ordinary typo, indistinguishable from a
  // deliberate key, and not something this check can catch.
  if (!key.includes("_") && /^[a-z]/.test(key)) {
    return true;
  }
  // Everything else mixes case with underscores (`video_URL`) or starts uppercase
  // (`Game_id`). heck would lowercase the word remainders and produce a different
  // key than this converter does, so the result would not exist on the Rust side.
  return false;
}

/** Diagnostics emitted while converting; the caller decides whether to surface them. */
export type IpcArgWarning =
  | { kind: "unsupported-key"; key: string }
  | { kind: "key-collision"; key: string; camel: string };

/**
 * Converts only the TOP-LEVEL keys of an IPC args object to lowerCamelCase.
 *
 * Nested objects and array elements are passed through untouched: their fields are
 * deserialized into Rust structs by serde, which uses the field names verbatim
 * (snake_case) because this repo sets no `rename_all` on those types.
 */
export function toIpcArgs(
  args?: Record<string, unknown> | null,
  onWarning?: (warning: IpcArgWarning) => void,
): Record<string, unknown> | undefined {
  // Normalize null to undefined: `invoke`'s `args = {}` default only applies to
  // undefined, so a null would be forwarded verbatim.
  if (args === undefined || args === null) {
    return undefined;
  }

  const transformed: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(args)) {
    if (onWarning && !isSupportedArgKey(key)) {
      onWarning({ kind: "unsupported-key", key });
    }
    const camel = snakeKeyToCamel(key);
    if (onWarning && Object.prototype.hasOwnProperty.call(transformed, camel)) {
      onWarning({ kind: "key-collision", key, camel });
    }
    transformed[camel] = value;
  }
  return transformed;
}
