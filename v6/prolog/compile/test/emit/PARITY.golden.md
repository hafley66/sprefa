# JSON Schema/OpenAPI parity

| feature | dialect | status | receipt |
| --- | --- | --- | --- |
| $defs | jsonschema | emits | module_defs/4 renders named relations below `$defs`. |
| $id | jsonschema | emits | entry module name and hash render as `name#hash`. |
| $ref via declared type name | jsonschema | emits | a declared column type renders a `$ref` into `$defs`. |
| $ref via rel-typed column | jsonschema | emits | a relational column renders a `$ref` into `$defs`. |
| additionalProperties | jsonschema | emits | relation objects render `additionalProperties: false`. |
| tagged option (catalog) | jsonschema | no_surface | option(T) schema rows are emitted from the catalog type path, not this dl6 fixture. |
| array items | jsonschema | no_surface | list(T) is not accepted by the current inline compiler door. |
| const | jsonschema | no_surface | no const literal or schema keyword exists in the dl6 surface. |
| enum | jsonschema | no_surface | no enum schema emission path exists in the current emitter. |
| format | jsonschema | no_surface | no format annotation surface exists in the current emitter. |
| integer | jsonschema | emits | `int` renders `type: integer`. |
| maximum | jsonschema | deferred-@ | annotation_at_curry; user 2026-08-10: constraints are @ stuff. |
| minimum | jsonschema | deferred-@ | annotation_at_curry; user 2026-08-10: constraints are @ stuff. |
| multipleOf | jsonschema | deferred-@ | annotation_at_curry; user 2026-08-10: constraints are @ stuff. |
| number | jsonschema | no_surface | the current compiled schema fixture has no float-valued column. |
| object | jsonschema | emits | each relation renders `type: object`. |
| oneOf/discriminated union | jsonschema | no_surface | no variant or oneOf surface exists in the current emitter. |
| pattern | jsonschema | deferred-@ | annotation_at_curry; user 2026-08-10: constraints are @ stuff. |
| patternProperties | jsonschema | no_surface | no patternProperties annotation or emission path exists. |
| prefixItems | jsonschema | no_surface | no prefixItems list surface or emission path exists. |
| properties | jsonschema | emits | relation columns render under `properties`. |
| recursive $ref | jsonschema | no_surface | type_cycle_witness rejects cyclic declared types before emission. |
| required | jsonschema | emits | non-option columns render in the `required` array. |
| string | jsonschema | emits | `text` renders `type: string`. |
| callbacks | openapi | no_surface | no callback declaration or route callback metadata exists. |
| components.schemas | openapi | emits | the loaded relation shapes render below `components.schemas`. |
| examples | openapi | no_surface | no example declaration or emitter input exists. |
| parameters | openapi | emits | path parameters from the served route table render as parameters. |
| paths | openapi | emits | api_route/5 facts render the served path table. |
| requestBody | openapi | no_surface | served routes have no request body schema metadata. |
| responses | openapi | emits | operation response status objects render from operation_responses/2. |
| securitySchemes | openapi | runtime_only | auth declarations depend on the served authentication policy. |
| webhooks | openapi | no_surface | no webhook route declaration or emitter input exists. |
