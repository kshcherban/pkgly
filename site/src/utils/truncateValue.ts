// ABOUTME: Pure helpers for truncating long hash/digest strings for display.
// ABOUTME: Keeps the full value available for copy/tooltip while showing a compact form.

const DEFAULT_HEAD = 12;
const DEFAULT_TAIL = 8;

/** Truncates a string in the middle once it exceeds head + tail + 1 chars. */
export function truncateMiddle(value: string, head = DEFAULT_HEAD, tail = DEFAULT_TAIL): string {
  if (!value || value.length <= head + tail + 1) {
    return value;
  }
  return `${value.slice(0, head)}…${value.slice(-tail)}`;
}

const HASH_PATTERN = /^(sha\d+:)?[0-9a-fA-F]{40,}$/;

/** True for sha256:/sha512: digests and bare hex hashes (40+ chars). */
export function isHashLike(value: string): boolean {
  return Boolean(value) && HASH_PATTERN.test(value);
}
