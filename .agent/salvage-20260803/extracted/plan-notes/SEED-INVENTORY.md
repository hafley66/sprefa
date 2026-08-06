# Seed inventory — hand-transcribed spine facts (step 1)

Source: `v6/sprefa-store/src/spine.rs`. 9 tables, 37 columns, 5 secondary indexes,
14 FKs. These are the exact `3a_spine_schema_facts.pl` facts for the implementer;
STEP numbers are the Pk position for composite PKs, `none` otherwise.

## table/2

```
table(strings,false).
table(repos,false).
table(roots,false).
table(repo_revs,false).
table(files,false).
table(revs_files,true).
table(file_bytes,true).
table(node,false).
table(edge,true).
```

## column/5 + inline single-col UNIQUE

BaseType from rust field type: `i64`->`integer`, `i32`->`int32`, `String`->`text`,
`Vec<u8>`->`blob`; nullable from `Option<T>`. PK from `#[sea_orm(primary_key)]`;
`auto_increment=false` shown where explicit (rowid-alias PKs keep auto). Inline
`UNIQUE` from `#[sea_orm(unique)]` (single-col, emitted inline by sea-orm — noted,
not added to index/4).

```
% strings (spine.rs:90-99)   inline UNIQUE: content
column(strings,string_id,integer,false,pos(1)).            % primary_key, auto_increment=false
column(strings,content,text,false,none).                   % unique

% repos (spine.rs:113-122)   inline UNIQUE: slug
column(repos,repo_id,integer,false,pos(1)).                % primary_key (auto)
column(repos,slug,text,false,none).                        % unique
column(repos,root,text,false,none).
column(repos,url,text,false,none).

% roots (spine.rs:136-144)   inline UNIQUE: path_string_id
column(roots,root_id,integer,false,pos(1)).                % primary_key (auto)
column(roots,repo_id,integer,false,none).
column(roots,path_string_id,integer,false,none).           % unique

% repo_revs (spine.rs:170-184)
column(repo_revs,rev_id,integer,false,pos(1)).             % primary_key (auto)
column(repo_revs,repo_id,integer,false,none).
column(repo_revs,kind,int32,false,none).
column(repo_revs,git_sha,blob,true,none).                  % Option<Vec<u8>>
column(repo_revs,root_id,integer,true,none).               % Option<i64>
column(repo_revs,base_rev_id,integer,true,none).           % Option<i64>

% files (spine.rs:215-224)   inline UNIQUE: content_hash
column(files,file_id,integer,false,pos(1)).                % primary_key (auto)
column(files,content_hash,blob,false,none).                % unique
column(files,size,integer,false,none).
column(files,lines,integer,false,none).

% revs_files (spine.rs:238-246)  WITHOUT ROWID, composite PK
column(revs_files,rev_id,integer,false,pos(1)).            % primary_key, auto_increment=false
column(revs_files,path_string_id,integer,false,pos(2)).    % primary_key, auto_increment=false
column(revs_files,file_id,integer,false,none).

% file_bytes (spine.rs:277-286)  WITHOUT ROWID, composite PK
column(file_bytes,file_id,integer,false,pos(1)).           % primary_key, auto_increment=false
column(file_bytes,start,integer,false,pos(2)).             % primary_key, auto_increment=false
column(file_bytes,end,integer,false,pos(3)).               % primary_key, auto_increment=false
column(file_bytes,string_id,integer,true,none).            % Option<i64>

% node (spine.rs:313-324)
column(node,node_id,integer,false,pos(1)).                 % primary_key (auto)
column(node,family,int32,false,none).
column(node,file_id,integer,false,none).
column(node,byte_start,integer,false,none).
column(node,byte_len,integer,false,none).
column(node,kind,int32,false,none).
column(node,name_id,integer,true,none).                    % Option<i64>

% edge (spine.rs:350-361)  WITHOUT ROWID, composite PK
column(edge,family,int32,false,pos(1)).                    % primary_key, auto_increment=false
column(edge,src_id,integer,false,pos(2)).                  % primary_key, auto_increment=false
column(edge,dst_id,integer,false,pos(3)).                  % primary_key, auto_increment=false
column(edge,kind,int32,false,pos(4)).                      % primary_key, auto_increment=false
```

## index/4 (spine.rs:440-472)

```
index(ux_repo_revs_identity,repo_revs,[repo_id,git_sha],[unique]).
index(ux_node_identity,node,[family,file_id,byte_start,kind],[unique]).
index(ix_revs_files_by_file,revs_files,[file_id],[]).
index(ix_edge_by_dst,edge,[family,dst_id],[]).
index(ux_repo_revs_work_root,repo_revs,[root_id],[unique,partial('kind = 1')]).  % spine.rs:463-465
```

## fk/4 (RelationTrait `belongs_to` edges)

```
fk(roots,repo_id,repos,repo_id).            % spine.rs:153
fk(roots,path_string_id,strings,string_id). % spine.rs:157
fk(repo_revs,repo_id,repos,repo_id).        % spine.rs:195
fk(repo_revs,root_id,roots,root_id).        % spine.rs:199
fk(repo_revs,base_rev_id,repo_revs,rev_id). % spine.rs:203 (self)
fk(revs_files,rev_id,repo_revs,rev_id).     % spine.rs:255
fk(revs_files,path_string_id,strings,string_id). % spine.rs:259
fk(revs_files,file_id,files,file_id).       % spine.rs:263
fk(file_bytes,file_id,files,file_id).       % spine.rs:291
fk(file_bytes,string_id,strings,string_id). % spine.rs:296
fk(node,file_id,files,file_id).             % spine.rs:332
fk(node,name_id,strings,string_id).         % spine.rs:336
fk(edge,src_id,node,node_id).               % spine.rs:369
fk(edge,dst_id,node,node_id).               % spine.rs:373
```

## Verification count

37 = 2+4+3+6+4+3+4+7+4; 14 FKs; 5 secondary indexes. Matches `table_names()`
(`src/spine.rs:386-399`) and `secondary_indexes()` (`spine.rs:431-473`).
