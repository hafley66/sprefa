// Ground truth for the dead-module rail on TypeScript. Every module is
// labelled with what knip says and what the rail must say. knip reasons over
// the IMPORT graph; the rail reasons over called NAMES, so the two disagree in
// a way each label states.
import { exportedOne } from './livePub.ts';
import { helperOne } from './liveDeep.ts';
import { shelvedOne } from './importedNeverCalled.ts';

export const shelf = [shelvedOne];

export function entry(): number {
  return exportedOne() + helperOne();
}
