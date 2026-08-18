# Relation ID access

`Revision.id` declares the existing integer endpoint of `Revision`.

Within a bound row value, `FileRec.revision.id` projects that endpoint. The
ordinary `FileRec.revision` form follows the row and renders its value.

`list(Revision.id)` is refused as `list_of_relation_ids(Revision)`. Current
list member storage writes JSON text through `__str`; it has no typed endpoint
member representation. No list column or wrapper identity is added by this
fixture.
