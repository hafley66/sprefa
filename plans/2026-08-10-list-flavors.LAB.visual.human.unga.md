# List flavor lab, visual record

```text
list(text)                       entity -- dense member(index,value)
list_entity_dense_sequence(text) entity -- dense member(index,value)
                                      \-- owner(parent,list)
                                      \-- refcount(list,count)
list_interned_set(text)          content -- member(content,value-id)
                                      \-- value(id,value)
list_entity_linked_sequence(text) entity -- member(member,list,value)
                                      \-- link(before,after)
```

```text
                  sequence                         set
dense             owned/entity/interned            incoherent: positional order
linked            owned/entity/interned            incoherent: predecessor order
unordered         incoherent: no sequence order    owned/entity/interned
```

Each template reaches the same artifact lowering and canonical-name path.
The schema declarations differ where the storage identity differs: junction
and refcount for shared entity lists, integer value dictionary ids for the
interned set, and adjacency rows for linked order.

