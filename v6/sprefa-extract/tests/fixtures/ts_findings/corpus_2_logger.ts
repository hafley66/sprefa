// The unrelated free `log` that `corpus_2.ts`'s `console.log` wrongly binds to.
export function log(message: string): void {
  process.stdout.write(message);
}
