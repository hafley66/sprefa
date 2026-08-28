// The rename anchor. The string literal, this comment, and the shadowed
// binding inside runShadowed all spell oldName and must survive untouched.

function newName(input: number): number {
  return input + 1;
}

const viaCall = newName(1);

const viaValue: (input: number) => number = newName;

function runShadowed(): number {
  const oldName = 41;
  return oldName + 1;
}

export const registryKey = "oldName";

export const total = viaCall + viaValue(2) + newName(3) + runShadowed();
