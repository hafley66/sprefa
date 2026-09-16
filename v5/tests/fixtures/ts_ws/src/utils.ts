export function formatName(name: string): string {
  return name.trim().toUpperCase();
}

// Same bare name as api.ts's helper() -- never imported from here by anyone.
export function helper(): string {
  return "utils-helper";
}
