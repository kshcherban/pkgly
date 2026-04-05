export function isAdvancedQuery(value: string): boolean {
  return /[a-zA-Z]+:/.test(value.trim());
}

export function shouldFetchPackages(value: string): boolean {
  const trimmed = value.trim();
  if (trimmed.length >= 2) {
    return true;
  }
  return isAdvancedQuery(trimmed);
}

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) {
    return "0 B";
  }
  const units = ["B", "KB", "MB", "GB", "TB"];
  const unitIndex = Math.min(
    units.length - 1,
    Math.floor(Math.log(bytes) / Math.log(1024)),
  );
  const value = bytes / Math.pow(1024, unitIndex);
  return `${value.toFixed(unitIndex === 0 ? 0 : 2)} ${units[unitIndex]}`;
}
