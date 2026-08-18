# Relation ID access

`Revision.id` declares the existing integer endpoint of `Revision`.

Within a bound row value, `FileRec.revision.id` projects that endpoint. The
ordinary `FileRec.revision` form follows the row and renders its value.

`list(Revision.id)` stores ordered local relation endpoints in the generated
member relation without string interning. Spreading the list binds the stored
endpoint. A `Revision.id` destination keeps the integer; a `Revision`
destination follows and renders the target row. No additional list identity or
member column is introduced.
