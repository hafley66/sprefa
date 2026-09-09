-- Generated from schema/1_facts.tsp by just gen.
CREATE TABLE IF NOT EXISTS "protocol" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "version" INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS "run" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "run" INTEGER NOT NULL,
    "mode" TEXT NOT NULL,
    "tool" TEXT NOT NULL,
    "version" TEXT NOT NULL,
    "scope" TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS "fact" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "fact" INTEGER NOT NULL,
    "relation" TEXT NOT NULL,
    "args" TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS "witness" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "fact" INTEGER NOT NULL,
    "run" INTEGER NOT NULL,
    "method" TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS "coverage" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "run" INTEGER NOT NULL,
    "relation" TEXT NOT NULL,
    "coverage" TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS "diagnostic" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "run" INTEGER NOT NULL,
    "relation" TEXT NOT NULL,
    "detail" TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS "node" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "fact" INTEGER,
    "family" TEXT NOT NULL,
    "span__start" INTEGER NOT NULL,
    "span__end" INTEGER NOT NULL,
    "kind" TEXT NOT NULL,
    "name" TEXT
);

CREATE TABLE IF NOT EXISTS "edge" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "fact" INTEGER,
    "family" TEXT NOT NULL,
    "kind" TEXT NOT NULL,
    "from__start" INTEGER NOT NULL,
    "from__end" INTEGER NOT NULL,
    "from_kind" TEXT,
    "to__start" INTEGER NOT NULL,
    "to__end" INTEGER NOT NULL,
    "to_kind" TEXT
);

CREATE TABLE IF NOT EXISTS "param" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "fact" INTEGER,
    "family" TEXT NOT NULL,
    "span__start" INTEGER NOT NULL,
    "span__end" INTEGER NOT NULL,
    "pos" INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS "arg" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "fact" INTEGER,
    "family" TEXT NOT NULL,
    "call__start" INTEGER NOT NULL,
    "call__end" INTEGER NOT NULL,
    "pos" INTEGER NOT NULL,
    "arg__start" INTEGER NOT NULL,
    "arg__end" INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS "df_field" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "fact" INTEGER,
    "family" TEXT NOT NULL,
    "owner__start" INTEGER NOT NULL,
    "owner__end" INTEGER NOT NULL,
    "name" TEXT NOT NULL,
    "value__start" INTEGER NOT NULL,
    "value__end" INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS "df_lit" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "fact" INTEGER,
    "family" TEXT NOT NULL,
    "node__start" INTEGER NOT NULL,
    "node__end" INTEGER NOT NULL,
    "kind" TEXT NOT NULL,
    "text" TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS "df_loop" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "fact" INTEGER,
    "family" TEXT NOT NULL,
    "span__start" INTEGER NOT NULL,
    "span__end" INTEGER NOT NULL,
    "var" TEXT,
    "collection" TEXT
);

CREATE TABLE IF NOT EXISTS "df_nest" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "fact" INTEGER,
    "family" TEXT NOT NULL,
    "call__start" INTEGER NOT NULL,
    "call__end" INTEGER NOT NULL,
    "loop__start" INTEGER NOT NULL,
    "loop__end" INTEGER NOT NULL,
    "depth" INTEGER NOT NULL,
    "collection" TEXT
);

CREATE TABLE IF NOT EXISTS "df_allocates" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "fact" INTEGER,
    "family" TEXT NOT NULL,
    "owner__start" INTEGER NOT NULL,
    "owner__end" INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS "sig" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "fact" INTEGER,
    "family" TEXT NOT NULL,
    "owner__start" INTEGER NOT NULL,
    "owner__end" INTEGER NOT NULL,
    "owner_start" INTEGER NOT NULL,
    "owner_end" INTEGER NOT NULL,
    "slot" TEXT NOT NULL,
    "pos" INTEGER NOT NULL,
    "ty" TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS "site" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "fact" INTEGER,
    "family" TEXT NOT NULL,
    "span__start" INTEGER NOT NULL,
    "span__end" INTEGER NOT NULL,
    "callee" TEXT NOT NULL,
    "callee_path" TEXT
);

CREATE TABLE IF NOT EXISTS "const" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "fact" INTEGER,
    "family" TEXT NOT NULL,
    "owner__start" INTEGER NOT NULL,
    "owner__end" INTEGER NOT NULL,
    "field" TEXT,
    "text" TEXT NOT NULL,
    "kind" TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS "doc" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "fact" INTEGER,
    "family" TEXT NOT NULL,
    "owner__start" INTEGER NOT NULL,
    "owner__end" INTEGER NOT NULL,
    "parent" TEXT,
    "text" TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS "doc_tag" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "fact" INTEGER,
    "family" TEXT NOT NULL,
    "owner__start" INTEGER NOT NULL,
    "owner__end" INTEGER NOT NULL,
    "tag" TEXT NOT NULL,
    "arg" TEXT,
    "text" TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS "doc_node" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "fact" INTEGER,
    "family" TEXT NOT NULL,
    "span__start" INTEGER NOT NULL,
    "span__end" INTEGER NOT NULL,
    "kind" TEXT NOT NULL,
    "name" TEXT NOT NULL,
    "parent" TEXT,
    "target" TEXT,
    "title" TEXT,
    "body__start" INTEGER,
    "body__end" INTEGER
);

CREATE TABLE IF NOT EXISTS "data_doc" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "fact" INTEGER,
    "family" TEXT NOT NULL,
    "ordinal" INTEGER NOT NULL,
    "span__start" INTEGER NOT NULL,
    "span__end" INTEGER NOT NULL,
    "format" TEXT NOT NULL,
    "doc" TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS "data_value" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "fact" INTEGER,
    "family" TEXT NOT NULL,
    "ordinal" INTEGER NOT NULL,
    "path" TEXT NOT NULL,
    "kind" TEXT NOT NULL,
    "text" TEXT,
    "span__start" INTEGER NOT NULL,
    "span__end" INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS "specifier" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "fact" INTEGER,
    "family" TEXT NOT NULL,
    "span__start" INTEGER NOT NULL,
    "span__end" INTEGER NOT NULL,
    "name" TEXT NOT NULL,
    "kind" TEXT NOT NULL,
    "module" TEXT,
    "imported" TEXT
);

CREATE TABLE IF NOT EXISTS "method_owner" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "fact" INTEGER,
    "family" TEXT NOT NULL,
    "owner__start" INTEGER NOT NULL,
    "owner__end" INTEGER NOT NULL,
    "self_type" TEXT,
    "trait" TEXT
);

CREATE TABLE IF NOT EXISTS "cfg_scope" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "fact" INTEGER,
    "family" TEXT NOT NULL,
    "span__start" INTEGER NOT NULL,
    "span__end" INTEGER NOT NULL,
    "cfg" TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS "test_only_call" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "fact" INTEGER,
    "family" TEXT NOT NULL,
    "callee" TEXT NOT NULL,
    "cfg" TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS "macro_site" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "family" TEXT NOT NULL,
    "span__start" INTEGER NOT NULL,
    "span__end" INTEGER NOT NULL,
    "macro_name" TEXT NOT NULL,
    "source" TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS "reference" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "fact" INTEGER,
    "family" TEXT NOT NULL,
    "span__start" INTEGER NOT NULL,
    "span__end" INTEGER NOT NULL,
    "functor" TEXT NOT NULL,
    "position" TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS "unresolved" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "family" TEXT NOT NULL,
    "path" TEXT,
    "span__start" INTEGER NOT NULL,
    "span__end" INTEGER NOT NULL,
    "reason" TEXT NOT NULL,
    "detail" TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS "projectedge" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "family" TEXT NOT NULL,
    "kind" TEXT NOT NULL,
    "from__start" INTEGER NOT NULL,
    "from__end" INTEGER NOT NULL,
    "to_blob" TEXT NOT NULL,
    "to__start" INTEGER NOT NULL,
    "to__end" INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS "flow_edge" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "family" TEXT NOT NULL,
    "kind" TEXT NOT NULL,
    "from_blob" TEXT NOT NULL,
    "from__start" INTEGER NOT NULL,
    "from__end" INTEGER NOT NULL,
    "to_blob" TEXT NOT NULL,
    "to__start" INTEGER NOT NULL,
    "to__end" INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS "resolved_edge" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "fact" INTEGER,
    "caller_path" TEXT NOT NULL,
    "caller_name" TEXT,
    "callee_path" TEXT NOT NULL,
    "callee_name" TEXT,
    "caller_site_start" INTEGER NOT NULL,
    "caller_site_end" INTEGER NOT NULL,
    "kind" TEXT NOT NULL,
    "resolution_origin" TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS "resolved_type_edge" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "fact" INTEGER,
    "owner_path" TEXT NOT NULL,
    "owner_name" TEXT,
    "owner_start" INTEGER NOT NULL,
    "owner_end" INTEGER NOT NULL,
    "target_path" TEXT NOT NULL,
    "target_name" TEXT,
    "kind" TEXT NOT NULL,
    "resolution_origin" TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS "resolved_import" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "src_path" TEXT NOT NULL,
    "name" TEXT NOT NULL,
    "local" TEXT NOT NULL,
    "target_path" TEXT NOT NULL,
    "target_name" TEXT,
    "kind" TEXT NOT NULL,
    "hops" INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS "file_edge" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "src_path" TEXT NOT NULL,
    "dst_path" TEXT NOT NULL,
    "kind" TEXT NOT NULL,
    "symbols" INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS "file_unresolved" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "src_path" TEXT NOT NULL,
    "module" TEXT NOT NULL,
    "reason" TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS "package_edge" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "src_manifest" TEXT NOT NULL,
    "dst_manifest" TEXT NOT NULL,
    "kind" TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS "file" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "path" TEXT NOT NULL,
    "digest" TEXT NOT NULL,
    "bytes" INTEGER NOT NULL,
    "lines" INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS "size_skip" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "path" TEXT NOT NULL,
    "bytes" BLOB NOT NULL,
    "limit" BLOB NOT NULL,
    "reason" TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS "capture" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "query" TEXT NOT NULL,
    "capture" TEXT NOT NULL,
    "text" TEXT NOT NULL,
    "start" INTEGER NOT NULL,
    "end" INTEGER NOT NULL,
    "match_start" INTEGER NOT NULL,
    "match_end" INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS "scip_def" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "symbol" TEXT NOT NULL,
    "file" TEXT NOT NULL,
    "repo" TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS "scip_name" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "symbol" TEXT NOT NULL,
    "name" TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS "scip_ref" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "file" TEXT NOT NULL,
    "symbol" TEXT NOT NULL,
    "def_file" TEXT NOT NULL,
    "repo" TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS "scip_edge" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "src" TEXT NOT NULL,
    "dst" TEXT NOT NULL,
    "repo" TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS "scip_fn_edge" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "caller" TEXT NOT NULL,
    "callee" TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS "scip_callee_type" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "sym" TEXT NOT NULL,
    "type" TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS "scip_local" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "fn" TEXT NOT NULL,
    "name" TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS "scip_impl" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "impl" TEXT NOT NULL,
    "iface" TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS "scip_index" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "reused" INTEGER NOT NULL,
    "tool_name" TEXT NOT NULL,
    "tool_version" TEXT NOT NULL,
    "documents" INTEGER NOT NULL,
    "index_mtime_unix_ms" BLOB,
    "staleness" TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS "scip_skip" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "lang" TEXT NOT NULL,
    "bin" TEXT NOT NULL,
    "reason" TEXT NOT NULL,
    "detail" TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS "scip_occurrence" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "path" TEXT NOT NULL,
    "symbol" TEXT NOT NULL,
    "start" INTEGER NOT NULL,
    "end" INTEGER NOT NULL,
    "roles" INTEGER NOT NULL,
    "definition" INTEGER NOT NULL,
    "import" INTEGER NOT NULL,
    "write_access" INTEGER NOT NULL,
    "read_access" INTEGER NOT NULL,
    "generated" INTEGER NOT NULL,
    "test" INTEGER NOT NULL,
    "forward_definition" INTEGER NOT NULL,
    "syntax_kind" INTEGER NOT NULL,
    "enclosing_start" INTEGER,
    "enclosing_end" INTEGER,
    "text" TEXT
);

CREATE TABLE IF NOT EXISTS "scip_occurrence_doc" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "path" TEXT NOT NULL,
    "start" INTEGER NOT NULL,
    "end" INTEGER NOT NULL,
    "pos" INTEGER NOT NULL,
    "text" TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS "scip_diagnostic" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "path" TEXT NOT NULL,
    "start" INTEGER NOT NULL,
    "end" INTEGER NOT NULL,
    "severity" INTEGER NOT NULL,
    "code" TEXT NOT NULL,
    "message" TEXT NOT NULL,
    "source" TEXT NOT NULL,
    "tags" TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS "scip_symbol" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "path" TEXT,
    "symbol" TEXT NOT NULL,
    "display_name" TEXT NOT NULL,
    "kind" INTEGER NOT NULL,
    "enclosing_symbol" TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS "scip_documentation" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "symbol" TEXT NOT NULL,
    "pos" INTEGER NOT NULL,
    "text" TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS "scip_signature" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "symbol" TEXT NOT NULL,
    "language" TEXT NOT NULL,
    "text" TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS "scip_signature_occurrence" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "symbol" TEXT NOT NULL,
    "ref_symbol" TEXT NOT NULL,
    "start" INTEGER NOT NULL,
    "end" INTEGER NOT NULL,
    "roles" INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS "scip_metadata" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "version" INTEGER NOT NULL,
    "tool_name" TEXT NOT NULL,
    "tool_version" TEXT NOT NULL,
    "tool_arguments" TEXT NOT NULL,
    "project_root" TEXT NOT NULL,
    "text_document_encoding" INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS "scip_document" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "path" TEXT NOT NULL,
    "language" TEXT NOT NULL,
    "position_encoding" INTEGER NOT NULL,
    "text" TEXT
);

CREATE TABLE IF NOT EXISTS "scip_relationship" (
    "_row" INTEGER PRIMARY KEY AUTOINCREMENT,
    "_input_path" TEXT,
    "_content_id" TEXT,
    "record" TEXT NOT NULL,
    "symbol" TEXT NOT NULL,
    "related_symbol" TEXT NOT NULL,
    "is_reference" INTEGER NOT NULL,
    "is_implementation" INTEGER NOT NULL,
    "is_type_definition" INTEGER NOT NULL,
    "is_definition" INTEGER NOT NULL
);

