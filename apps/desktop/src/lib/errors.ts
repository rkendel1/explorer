/** Shape of a failed Tauri command's rejection (see `CommandError` in Rust). */
export interface CommandError {
  code: string;
  message: string;
}

function isCommandError(value: unknown): value is CommandError {
  return (
    typeof value === 'object' &&
    value !== null &&
    'message' in value &&
    typeof (value as { message: unknown }).message === 'string'
  );
}

/** Extract a human-readable message from a failed `invoke()` call. */
export function errorMessage(error: unknown): string {
  if (isCommandError(error)) {
    return error.message;
  }
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}
