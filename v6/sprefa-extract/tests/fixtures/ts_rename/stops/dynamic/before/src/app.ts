class Foo {}

const viaComputed = { mark: 1 }["Foo"];

import("./m").then(module => {
  const viaDynamic = module.Foo;
  return viaDynamic;
});

export const used = new Foo();
