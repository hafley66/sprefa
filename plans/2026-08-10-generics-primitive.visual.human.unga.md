# Generics primitive lab, plain version

The compiler now has an early generic-expansion step.  It finds a list inside
an option type, makes one generated relation for that list type, and then lets
the existing option code treat that generated relation like any other relation.

The generated list relation is only a lab placeholder.  It has an id and a
text value.  It does not choose list storage, ordering, ownership, or runtime
list behavior.

Names are stable text.  The visible part says what the type looks like, and a
16-character SHA-256 prefix distinguishes nested boundaries.  The compiler
checks the resulting name against author names before it creates declarations.

The lab ran two template versions on the same full fixture.  One version used
typed artifact records and one used raw declaration terms.  Both produced 39
declarations and the same canonical declaration text.  The typed-record
version remains in the compiler.

The list template has no generated-template dependency.  Its dependency-topo
order with canonical tie-break has no edges, so its order is the same canonical
sort used by the retained code.

The one new fixture drives enum expansion, scalar options, an option relation
companion, the nested list placeholder, derived author relations, arrivals,
and retractions through the oracle.  The `.golden` file shows the complete
expanded program term.

The full fixture also reverses its declaration list and runs expansion again.
Both expanded terms serialize to the same bytes.  The focused compiler test
names this receipt `generic_e2e_declaration_permutation_is_byte_deterministic`.
