export function greet(name: string): string {
  const message = `hello ${name}`;
  // debug line left in on purpose (the fixture's rail target):
  console.log(message);
  return message;
}
