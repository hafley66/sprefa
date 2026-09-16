% 3a_spine_schema_facts.pl: the spine catalog, as facts. Seed row interfaces
% and DDL transcribed from the live files (the three sources cited below are
% the receipts; a mismatch between these facts and the emitted ts is a bug in
% these facts, never a reason to change the ts).
%
% Sources (all cross-checked at seed time):
%   v6/sprefa-store/js/src/engine/spine.ts:62-102   the 7 row interfaces
%   v6/sprefa-store/js/src/engine/types.ts:80-95    NodeRow/EdgeRow interfaces
%   v6/sprefa-store/src/spine.rs:314-473            DDL, WithoutRowid, pk truth

:- module(spine_schema_facts,
          [ 'table'/2,
            table_symbol/2,
            column/6
          ]).

% table(Name, WithoutRowid).  WithoutRowid true only for the composite-key
% junctions (revs_files, file_bytes, edge), per spine.rs:414-417.

table(strings,     false).
table(repos,       false).
table(roots,       false).
table(repo_revs,   false).
table(files,       false).
table(revs_files,  true).
table(file_bytes,  true).
table(node,        false).
table(edge,        true).

% table_symbol(Table, ScipSymbol).  Synthetic SCIP ids in the typeir scheme
% (PLAN2 section 5, type-typedef form). INERT to ts output until step c wires
% the checker; shipped now so the column exists in the facts.

table_symbol(strings,     'typeir . spine dev strings#').
table_symbol(repos,       'typeir . spine dev repos#').
table_symbol(roots,       'typeir . spine dev roots#').
table_symbol(repo_revs,   'typeir . spine dev repo_revs#').
table_symbol(files,       'typeir . spine dev files#').
table_symbol(revs_files,  'typeir . spine dev revs_files#').
table_symbol(file_bytes,  'typeir . spine dev file_bytes#').
table_symbol(node,        'typeir . spine dev node#').
table_symbol(edge,        'typeir . spine dev edge#').

% column(Table, Name, BaseType, Nullable, Pk, ScipSymbol).
% BaseType closed set: integer int32 text blob. Pk = none | pos(N). Pks are
% never nullable. Column order in each table matches the ts interface order.

column(strings,    string_id,      integer, false, pos(1), 'typeir . spine dev strings#string_id.').
column(strings,    content,        text,    false, none,   'typeir . spine dev strings#content.').

column(repos,      repo_id,        integer, false, pos(1), 'typeir . spine dev repos#repo_id.').
column(repos,      slug,           text,    false, none,   'typeir . spine dev repos#slug.').
column(repos,      root,           text,    false, none,   'typeir . spine dev repos#root.').
column(repos,      url,            text,    false, none,   'typeir . spine dev repos#url.').

column(roots,      root_id,        integer, false, pos(1), 'typeir . spine dev roots#root_id.').
column(roots,      repo_id,        integer, false, none,   'typeir . spine dev roots#repo_id.').
column(roots,      path_string_id, integer, false, none,   'typeir . spine dev roots#path_string_id.').

column(repo_revs,  rev_id,         integer, false, pos(1), 'typeir . spine dev repo_revs#rev_id.').
column(repo_revs,  repo_id,        integer, false, none,   'typeir . spine dev repo_revs#repo_id.').
column(repo_revs,  kind,           int32,   false, none,   'typeir . spine dev repo_revs#kind.').
column(repo_revs,  git_sha,        blob,    true,  none,   'typeir . spine dev repo_revs#git_sha.').
column(repo_revs,  root_id,        integer, true,  none,   'typeir . spine dev repo_revs#root_id.').
column(repo_revs,  base_rev_id,    integer, true,  none,   'typeir . spine dev repo_revs#base_rev_id.').

column(files,      file_id,        integer, false, pos(1), 'typeir . spine dev files#file_id.').
column(files,      content_hash,   blob,    false, none,   'typeir . spine dev files#content_hash.').
column(files,      size,           integer, false, none,   'typeir . spine dev files#size.').
column(files,      lines,          integer, false, none,   'typeir . spine dev files#lines.').

column(revs_files, rev_id,         integer, false, pos(1), 'typeir . spine dev revs_files#rev_id.').
column(revs_files, path_string_id, integer, false, pos(2), 'typeir . spine dev revs_files#path_string_id.').
column(revs_files, file_id,        integer, false, none,   'typeir . spine dev revs_files#file_id.').

column(file_bytes, file_id,        integer, false, pos(1), 'typeir . spine dev file_bytes#file_id.').
column(file_bytes, start,          integer, false, pos(2), 'typeir . spine dev file_bytes#start.').
column(file_bytes, end,            integer, false, pos(3), 'typeir . spine dev file_bytes#end.').
column(file_bytes, string_id,      integer, true,  none,   'typeir . spine dev file_bytes#string_id.').

column(node,       node_id,        integer, false, pos(1), 'typeir . spine dev node#node_id.').
column(node,       family,         int32,   false, none,   'typeir . spine dev node#family.').
column(node,       file_id,        integer, false, none,   'typeir . spine dev node#file_id.').
column(node,       byte_start,     integer, false, none,   'typeir . spine dev node#byte_start.').
column(node,       byte_len,       integer, false, none,   'typeir . spine dev node#byte_len.').
column(node,       kind,           int32,   false, none,   'typeir . spine dev node#kind.').
column(node,       name_id,        integer, true,  none,   'typeir . spine dev node#name_id.').

column(edge,       family,         int32,   false, pos(1), 'typeir . spine dev edge#family.').
column(edge,       src_id,         integer, false, pos(2), 'typeir . spine dev edge#src_id.').
column(edge,       dst_id,         integer, false, pos(3), 'typeir . spine dev edge#dst_id.').
column(edge,       kind,           int32,   false, pos(4), 'typeir . spine dev edge#kind.').
