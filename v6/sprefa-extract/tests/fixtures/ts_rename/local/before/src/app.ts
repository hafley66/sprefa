// The rename anchor. The string literal, this comment, and the shadowed
// binding inside runShadowed all spell oldName and must survive untouched.

function oldName(input: number): number {
  return input + 1;
}

const viaCall = oldName(1);

const viaValue: (input: number) => number = oldName;

function runShadowed(): number {
  const oldName = 41;
  return oldName + 1;
}

export const registryKey = "oldName";

export const total = viaCall + viaValue(2) + oldName(3) + runShadowed();
