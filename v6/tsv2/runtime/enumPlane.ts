import { of } from "rxjs";

import type { IArrivalBatch, IEnumPlane, IEnumRefColumns, IRowValue } from "./types.ts";

/**
 * A column typed by an enum name holds a REFERENCE: the referenced instance's
 * integer id, the same carrier `commit__reviewed_by(commit_id, person_id)`
 * uses for a rel-typed reference column. The value form is the variant
 * constructor at construction position, and the compiler lowers it there; it
 * never reaches this door. Both directions therefore carry the integer
 * unchanged, and the emitted `rel_declared_column_types` entry for such a
 * column is already `int`, so the arrival door's own `field_not_int` check is
 * the reference check.
 *
 * Receipts: conformance/fixtures/0_enum_variants.pl
 * enum_name_is_a_column_type feeds `picked(101, 401)` and reads
 * `picked: add [[101,401]]`; 17_recursive_enum.pl carries `tree_branch(2,1,3)`;
 * 0_option_type.pl option_text_column_reads_through_tag_join carries
 * `user_profile(1, 501)`.
 */
function check_reference(rel: string, refs: IEnumRefColumns[string], row: readonly IRowValue[]): void {
  refs.forEach((reference, index) => {
    if (reference === null || reference === undefined || index >= row.length) return;
    const value = row[index];
    if (typeof value !== "number" || !Number.isInteger(value)) {
      throw new Error(`enum_arrival_shape_mismatch: not_a_reference(${rel}, ${reference.name})`);
    }
  });
}

export const EnumPlane: IEnumPlane = {
  intern(_seam, _types, ref_columns, arrivals) {
    for (const arrival of arrivals) {
      const refs = ref_columns[arrival.rel];
      if (refs !== undefined) check_reference(arrival.rel, refs, arrival.row);
    }
    return of(arrivals as IArrivalBatch);
  },

  decode_deltas(_seam, _types, _ref_columns, _relations, deltas) {
    return of(deltas);
  },

  decode_rows(_seam, _types, _ref_columns, _relations, _rel, rows) {
    return of(rows);
  },
};
