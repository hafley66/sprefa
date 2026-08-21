// Ground truth for the dead-module rail on TypeScript. Every module is
// labelled with what knip says and what the rail must say. knip reasons over
// the IMPORT graph; the rail reasons over called NAMES and, in the crawl only,
// over resolved import edges, so the two disagree in a way each label states.
import { exportedOne } from './livePub.ts';
import { helperOne } from './liveDeep.ts';
import { shelvedOne } from './importedNeverCalled.ts';
import valueShelfDefault from './valueShelf.ts';

// Five forms whose target no call site anywhere names. Each one is an import
// edge and nothing else, so the crawl reaches its target only by resolving a
// specifier against the file set.
export * from './barrel.ts';
export * from './util';
export * from '@fixture/aliasedTarget.ts';
export { renamedOne as publicRenamed } from './renamedTarget.ts';

export const shelf = [shelvedOne];
export const bag = { valueShelfDefault };

export function entry(): number {
  return exportedOne() + helperOne();
}
