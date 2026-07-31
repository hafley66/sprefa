/**
 * Open a SQLite connection and boot a generated program's DDL.
 *
 * `open_db` is the store's single `@libsql/client`
 * constructor (sprefa-store-engine/src/engine/lib.ts — "the one constructor
 * for that connection"); `SqlRunner` is the store's single driver seam
 * (sprefa-store-engine/src/engine/sqlRunner.ts). Neither is redeclared
 * here. The v6/dl fact-plane schema is unrelated to a compiled tsv2
 * program's own tables, and `open_db` alone (no schema side effect) is
 * exactly the "open a connection" primitive this module needs.
 */

import type { Observable } from "rxjs";
import { open_db } from "sprefa-store-engine/src/engine/lib.ts";
import { SqlRunner } from "sprefa-store-engine/src/engine/sqlRunner.ts";

import type { IScratchStore, ISqlSeam } from "./types.ts";

export const ScratchStore: IScratchStore = {
  open(url: string): ISqlSeam {
    return { db: open_db(url), runner: SqlRunner };
  },

  boot(seam: ISqlSeam, ddl: readonly string[]): Observable<void> {
    // executeMultiple resolves void at the store seam.
    return seam.runner.executeMultiple(seam.db, ddl.join(";\n"));
  },
};
