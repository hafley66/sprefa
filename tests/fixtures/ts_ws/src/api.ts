export function fetchUser(id: number): string {
  return "user-" + id;
}

// Same bare name as utils.ts's helper() -- the ambiguous-name fixture case.
// Only service.ts imports THIS one.
export function helper(): string {
  return "api-helper";
}
