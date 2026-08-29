// Corpus finding, NOT FIXED: a decorator is a call and carries no `site` record.
// `CallWalker` handles CallExpression, NewExpression and JSXElement; oxc's
// `Decorator` node reaches none of them, so nothing binds `Service` to
// `Injectable` or `handle` to `Log`.
//
// Expected: two `site` records, callee "Injectable" and callee "Log".
// Observed: zero `site` records; the two functions appear only as call defs.
function Injectable(target: unknown): void {}
function Log(target: unknown, key: string): void {}

@Injectable
export class Service {
  @Log
  handle(input: string): string {
    return input;
  }
}
