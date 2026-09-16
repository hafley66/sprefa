% 0_facts.pl : three real spine tables transcribed as table/2 + column/8.
%
% Source of truth: v6/sprefa-store/src/spine.rs (strings 88-108, node 311-324,
% edge 348-361). Only the three tables the brief names are transcribed; the
% other six spine tables are out of scope for this lab.
%
% column/8 : column(Table, Column, Type, Nullable, RefTable, RefColumn,
%                   SourceOrder, Note)
%   Nullable:  true | false.  RefTable/RefColumn: none when the column has no
%   foreign key (the FK is the cross-file reference carrier). SourceOrder is
%   the column's position in the source struct, significant for field order.
%
% DELIBERATE CROSS-FILE references (the whole point of this lab):
%   - node.name_id  -> strings.string_id   (interned name, nullable)
%   - edge.src_id   -> node.node_id        (source graph node)
%   - edge.dst_id   -> node.node_id        (destination graph node)
%
% Under the ts/rust file split in 1_collect, node+edge land in the "graph"
% target file/module and strings in the "core" target file/module, so node's
% reference to strings crosses the file boundary and forces an import line
% that the renderer derives from ref/2, never hand-writes.

:- module(alloy_facts,
          [ (table)/2,
            column/8
          ]).

table(strings, string_table).
table(node,    graph_node).
table(edge,    graph_edge).

% ---- strings (dimension; self-contained, no outbound FK) -------------------
column(strings, string_id, i64,    false, none,   none,       1,
       'dense id assigned by the resident interner').
column(strings, content,   string, false, none,   none,       2,
       'interned utf8 text; unique').

% ---- node (unified graph node, content-scoped) ------------------------------
column(node, node_id,    i64,    false, none,   none,      1,
       'graph node primary key').
column(node, family,     i32,    false, none,   none,      2,
       'node family id').
column(node, file_id,    i64,    false, none,   none,      3,
       'document this node belongs to').
column(node, byte_start, i64,    false, none,   none,      4,
       'offset within file').
column(node, byte_len,   i64,    false, none,   none,      5,
       'span length').
column(node, kind,       i32,    false, none,   none,      6,
       'node kind').
column(node, name_id,    i64,    true,  strings, string_id, 7,
       'interned name id; NULL for anonymous nodes').

% ---- edge (unified graph edge; composite key, references node twice) --------
column(edge, family, i32, false, none, none,      1, 'edge family id').
column(edge, src_id, i64, false, node, node_id,   2, 'source node id').
column(edge, dst_id, i64, false, node, node_id,   3, 'destination node id').
column(edge, kind,   i32, false, none, none,      4, 'edge kind').
