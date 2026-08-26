import { thing } from './b.ts';
import { thing as bare } from './b';
import { fromDir } from './dir';
import { thing as emitted } from './b.js';
import { thing as aliased } from '@app/b';
import { main } from 'pkg-exports';
export { reexported } from './reexport';
export * from './dir';

const lazy = import('./b.js');
const legacy = require('./b');
import legacyEquals = require('./b');

export const uses = [thing, bare, fromDir, emitted, aliased, main, lazy, legacy, legacyEquals];
export const plain = 'hello ./b';
