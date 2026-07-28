/**
 * scratchStore.ts — opens a throwaway SQLite connection for one tsv2 run and
 * boots a generated program's DDL once.
 *
 * Reuse law: `open_db` is the store's own single `@libsql/client`
 * constructor (sprefa-store-engine/src/engine/lib.ts — "the one constructor
 * for that connection"); `SqlRunner` is the store's single driver seam
 * (sprefa-store-engine/src/engine/sqlRunner.ts). Neither is redeclared
 * here. `Store.open`/`create_all_tables` (the v6/dl fact-plane spine) are
 * deliberately NOT reused: that schema is unrelated to a compiled tsv2
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
    // `executeMultiple` is the store's own multi-statement DDL runner: "the
    // driver's own executeMultiple resolves nothing" (sqlRunner.ts), so
    // `Observable<void>` here is honest, not a thrown-away value.
    return seam.runner.executeMultiple(seam.db, ddl.join(";\n"));
  },
};
