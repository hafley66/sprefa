# Scan Pipeline Lifecycle

Every open has its close. State creation → mutation → destruction.

```
SCAN COMMAND
|
+- load_config()
|   ( read sprefa.toml -> Config )                          <- ephemeral, dropped at fn exit
|
+- build_scanner()
|   |
|   +- parse_program(text)
|   |   ( .sprf text -> Program[Vec<Statement>] )           <- in-memory only
|   |
|   +- lower_program(&Program)
|   |   ( Program -> RuleSet + Vec<DepEdge> )               <- in-memory only
|   |
|   +- compute_rule_hashes(&rules, &edges)
|   |   ( rules -> HashMap<name, {schema_hash, extract_hash}> )
|   |
|   +- store.create_rule_tables(specs, hashes)
|       FOR EACH rule:
|       |
|       +- compare hashes vs sprf_meta table
|       |   SchemaChanged? -> ( DROP TABLE {rule}_data )     <- DATA DESTROYED
|       |   ExtractChanged?-> ( DELETE FROM {rule}_data )    <- ROWS DESTROYED
|       |   Unchanged?     -> skip
|       |
|       +- ( CREATE TABLE IF NOT EXISTS {rule}_data         <- SCHEMA CREATED
|       |     {col}_ref INTEGER, {col}_str INTEGER,
|       |     repo_id, file_id, rev )
|       |
|       +- ( DROP VIEW + CREATE VIEW {rule} )               <- VIEWS RECREATED always
|       |   ( DROP VIEW + CREATE VIEW {rule}_refs )
|       |
|       +- ( INSERT INTO sprf_meta                          <- HASHES PERSISTED
|             (rule_name, schema_hash, extract_hash) )
|
+- scan_repo() per repo x rev
|   |
|   +- git ls-files -> file list
|   |
|   +- extractor.extract(bytes, path, ctx) per file
|   |   ( file bytes -> Vec<RawRef> )                        <- in-memory
|   |   ( RawRefs grouped by (rule_name, group) -> FileResult )
|   |
|   +- store.flush_batch(repo, rev, files)   <- SINGLE TX
|       |
|       +- ( INSERT OR IGNORE INTO strings )                <- INTERNED
|       |   ( SELECT id back -> HashMap<value, id> )
|       |
|       +- ( INSERT INTO files ON CONFLICT UPDATE )         <- UPSERTED
|       |   ( SELECT id back -> HashMap<path, id> )
|       |
|       +- ( INSERT OR IGNORE INTO rev_files )              <- LINKED
|       |
|       +- per file, per rule:
|           +- ( INSERT OR IGNORE INTO refs )               <- SPANS STORED
|           |   ( SELECT id back -> ref_id )
|           |
|           +- ( INSERT INTO {rule}_data                    <- ROWS CREATED
|                 {col}_ref, {col}_str, repo_id,
|                 file_id, rev )
|       COMMIT
|
+- discovery loop (max 10 rounds)
    |
    +- query unscanned (repo, rev) pairs from {rule}_data
    |   where repo/rev columns have scan annotations
    |
    +- clone/fetch new repos if needed
    |
    +- scan_repo() again ----------------------+
        (same flush_batch path)                |
        loop until no new pairs ---------------+


DELETION (file removed from git diff):
|
+- delete_rev_files_by_paths(repo, branch, paths)  <- SINGLE TX
    +- ( CREATE TEMP _dead_files <- file IDs )
    +- ( DELETE FROM {kind}_data WHERE file_id IN _dead_files )  <- ROWS DESTROYED
    +- ( DELETE FROM refs WHERE file_id IN _dead_files )         <- SPANS DESTROYED
    +- ( DELETE FROM rev_files ... )                             <- LINKS DESTROYED
    +- ( DELETE FROM files WHERE id IN _dead_files )             <- FILES DESTROYED
    +- ( DROP TABLE _dead_files )
    COMMIT
```

## File-scoped namespaces (planned)

```
+- build_scanner()
    |
    +- FOR EACH .sprf file:
    |   +- parse_program(text) -> Program
    |   +- lower_program(&Program) -> RuleSet (rules carry source filename)
    |
    +- ( ATTACH DATABASE ':memory:' AS {filename} )         <- SCHEMA CREATED per .sprf file
    |
    +- store.create_rule_tables(specs, hashes)
        tables go into {filename}.{rule}_data               <- NAMESPACED
        views go into {filename}.{rule}

Resolution:
  bare `rule_name`      -> current file's schema            {filename}.{rule}
  dotted `file.rule`    -> explicit cross-file              {file}.{rule}

Teardown:
  ( DETACH DATABASE {filename} )                            <- SCHEMA DESTROYED
  only needed if .sprf file removed or renamed
```

## State summary

| Phase | Creates | Destroys |
|-------|---------|----------|
| create_rule_tables | {rule}_data tables, views, sprf_meta rows | tables (schema change), rows (extract change) |
| flush_batch | strings, files, refs, rev_files, {rule}_data rows | nothing (insert-only within tx) |
| delete_rev_files | nothing | {kind}_data rows, refs, rev_files, files |
| rename_file_paths | nothing | nothing (UPDATE path/stem/ext in place) |
