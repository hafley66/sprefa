# Prover namespaces, proposed

Columns from the current prelude declarations
(`prelude/1_declarations.dl7`, `prelude/4_type_algebra.dl7`). Members keep
their names minus a prefix the namespace now carries. Not applied; needs
Chris's OK. Probe that the form works:
`plans/v8/probes/namespace/0_member_products.dl7`.

```lisp
(: proof
   (* (: extends            (* (: source type) (: contract type) (: return type)))
      (: extends_all        (* (: source type) (: contracts any) (: return type)))
      (: asked              (* (: source type) (: contract type)))
      (: asked_all          (* (: source type) (: contracts any)))
      (: candidate          (* (: source type) (: contract type)))
      (: fails              (* (: source type) (: contract type)))
      (: fails_at           (* (: source type) (: contract type) (: name any) (: reason proof.Reason)))
      (: undecided          (* (: source type) (: contract type)))
      (: diagnostic         (* (: source type) (: contract type) (: name any) (: reason proof.Reason)))
      (: reason_label       (* (: label str) (: reason proof.Reason)))
      (: checker_of         (* (: bound type) (: checker any)))
      (: has_checker        (* (: bound type)))
      (: obligation         (* (: site any) (: parameter any) (: argument any) (: bound type)))
      (: discharged         (* (: site any) (: parameter any) (: argument any) (: result any)))
      (: missing_contract   (* (: source type) (: contracts any)))
      (: primitive_contract (* (: contract type)))
      (: primitive_mismatch (* (: source type) (: contract type)))
      (: same_pair          (* (: source type) (: contract type)))
      (: has_field          (* (: source type) (: name any)))
      (: implements         (* (: contract type) (: source type) (: witness type)))
      (: valid_impl         (* (: contract type) (: source type) (: witness type)))
      (: invalid_impl_edge  (* (: contract type) (: source type) (: witness type) (: name any) (: reason proof.Reason)))))

(: edge
   (* (: type    (* (: product type) (: name any) (: return type)))
      (: value   (* (: product type) (: name any) (: return type)))
      (: default (* (: product type) (: name any) (: return any)))
      (: seen    (* (: product type) (: name any) (: value any)))))

(: value
   (* (: type (* (: value any) (: return type)))))
```

| old | new |
|---|---|
| `Extends`, `ExtendsAll` | `proof.extends`, `proof.extends_all` |
| `extends_diagnostic`, `extends_reason`, `ExtendsReason` | `proof.diagnostic`, `proof.reason_label`, `proof.Reason` |
| other prover names | `proof.<same name>` |
| `column_type`, `column_value`, `default` | `edge.type`, `edge.value`, `edge.default` |
| new | `edge.seen`, `value.type` |

Same pattern pending for `curry_*`, `pick_*`, `exclude_*`, `intersection_*`,
`program_*`.
