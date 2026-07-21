//! Bijection gate for the `_sym_dict` storage normalization (2026-07-21,
//! plans/2026-07-21-sym-dict-correctness-proof.md). Every interned rel column
//! now stores a dense `_sym_dict` surrogate instead of the 8-byte StringId
//! hash. This test is the MECHANICAL proof that the surrogate is behavior
//! preserving on a real corpus — it does not trust a green suite, it counts:
//!
//!  1. `_sym_dict` is a bijection: `COUNT(*) == COUNT(DISTINCT id) ==
//!     COUNT(DISTINCT sym_hash)`, and every `id` is dense (`MAX(id) ==
//!     COUNT(*)`, no gaps on a fresh corpus).
//!  2. Per sym-bearing rel, `COUNT(DISTINCT sym) == COUNT(DISTINCT sym_txt)`:
//!     the dense id partitions the corpus EXACTLY as the decoded text does, so
//!     no two symbols merged and none split.
//!  3. Atomicity: every stored interned cell is a valid dense id (`NOT IN
//!     _sym_dict` is empty) — nothing was left in raw-hash space to silently
//!     break an integer-equality join.
//!  4. Join parity: a cross-family sym join over the DENSE columns returns the
//!     same rowcount as the same join over the decoded TEXT columns.

use sprefa_v5::{db, engine::Engine, lex, parse};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const SRC: &str = r#"
pub struct Widget { pub n: i64 }

pub fn leaf() -> i64 { 1 }
pub fn helper() -> i64 { leaf() }
pub fn run() -> i64 { helper() + leaf() }
pub fn other() -> i64 { helper() }
"#;

// Extract the call + type families over the working tree, and bind a query
// head per rel so `call_rels_used` / type extraction gate on them.
const PROG: &str = r#"
rel seen(path: file).
seen(path) <- scan("WORK", "src/**/*.rs", path, rev).

# SQL write paths — the exact seams 624ba534 left in raw-hash space. `word` is a
# ground-fact rel (its sym literals lower through `sprf_sym_intern`); `echo_word`
# is a derived rel that passes the sym through. Both must land in the SAME dense
# id space as the router-written call/type rels, or the cross-path join below is
# empty.
rel word(w: sym).
word("alpha_symbol").
word("beta_symbol").
rel echo_word(w: sym).
echo_word(w) <- word(w).

? call_def(repo, sym, kind, file, line, end).
? call_edge(caller, callee, kind).
? type_entity(repo, sym, name, kind, parent, file, line).
? word(w).
? echo_word(w).
"#;

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .expect("git");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn build() -> (Engine, PathBuf) {
    let dir = std::env::temp_dir().join(format!("sym_dict_bijection_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("src/a.rs"), SRC).unwrap();
    git(&dir, &["init", "-q"]);
    git(&dir, &["config", "user.email", "t@example.com"]);
    git(&dir, &["config", "user.name", "T"]);
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-q", "-m", "base"]);

    let prog = parse::parse(lex::lex(PROG).unwrap()).unwrap();
    let conn = db::open(Some(dir.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, dir.clone());
    for _ in 0..4 {
        eng.tick(&prog, true).unwrap();
    }
    (eng, dir)
}

fn count(eng: &Engine, sql: &str) -> i64 {
    let rows = eng.query_sql(sql, &[]).unwrap();
    rows[0][0].as_i64().expect("count is an integer")
}

#[test]
fn sym_dict_is_a_dense_bijection_and_preserves_every_join() {
    let (eng, dir) = build();

    // 0. Non-vacuous: the corpus actually carries symbols across both families.
    let call_def_syms = count(&eng, "SELECT COUNT(DISTINCT sym) FROM rel_call_def");
    let type_syms = count(&eng, "SELECT COUNT(DISTINCT sym) FROM rel_type_entity");
    assert!(
        call_def_syms >= 4,
        "expected >=4 call_def syms (leaf/helper/run/other), got {call_def_syms}"
    );
    assert!(type_syms >= 1, "expected >=1 type_entity sym, got {type_syms}");

    // 1. `_sym_dict` is a dense bijection, seeded above the coord-id range.
    let dict_rows = count(&eng, "SELECT COUNT(*) FROM _sym_dict");
    let dict_ids = count(&eng, "SELECT COUNT(DISTINCT id) FROM _sym_dict");
    let dict_hashes = count(&eng, "SELECT COUNT(DISTINCT sym_hash) FROM _sym_dict");
    assert_eq!(dict_rows, dict_ids, "_sym_dict.id is not unique");
    assert_eq!(dict_rows, dict_hashes, "_sym_dict.sym_hash is not unique");
    // Contiguous ids (no gaps on a fresh build) — the allocator hands out
    // 1e9+1, 1e9+2, ... in order; there is no seed row.
    let span = count(&eng, "SELECT MAX(id) - MIN(id) + 1 FROM _sym_dict");
    assert_eq!(
        span, dict_rows,
        "_sym_dict ids are not contiguous (span {span} != count {dict_rows})"
    );
    // Disjoint from the dense `_df_node_dict` coordinate space: every real sym
    // id is >= 1e9, every df-coordinate id is far below it.
    let min_real = count(&eng, "SELECT MIN(id) FROM _sym_dict WHERE sym_hash != 0");
    assert!(
        min_real >= 1_000_000_001,
        "sym surrogates must be seeded >= 1e9+1 (min real id {min_real})"
    );
    let max_coord = count(&eng, "SELECT COALESCE(MAX(id), 0) FROM _df_node_dict");
    assert!(
        max_coord < 1_000_000_000,
        "coordinate ids must stay below the 1e9 sym base (max coord {max_coord})"
    );

    // 2. Per sym-bearing rel: dense-id partition == decoded-text partition.
    // (rel, dense column, its _txt decode column) — same column name both sides.
    let cases: &[(&str, &str)] = &[
        ("call_def", "sym"),
        ("type_entity", "sym"),
        ("call_edge", "caller"),
        ("call_edge", "callee"),
        ("type_edge", "\"from\""),
        ("type_edge", "\"to\""),
        ("call_name", "sym"),
        ("word", "w"),       // SQL ground-fact write path
        ("echo_word", "w"),  // SQL derived-rule write path
    ];
    for (rel, col) in cases {
        let dense = count(&eng, &format!("SELECT COUNT(DISTINCT {col}) FROM rel_{rel}"));
        let text = count(&eng, &format!("SELECT COUNT(DISTINCT {col}) FROM rel_{rel}_txt"));
        assert_eq!(
            dense, text,
            "bijection broken on {rel}.{col}: {dense} distinct dense ids vs {text} distinct texts"
        );
    }

    // 3. Atomicity: every non-sentinel interned cell is a valid dense id — no
    // raw hash was left in a sym column to silently empty an integer join.
    for (rel, col) in cases {
        let orphan = count(
            &eng,
            &format!(
                "SELECT COUNT(*) FROM rel_{rel} \
                 WHERE {col} IS NOT NULL AND {col} != 0 AND {col} NOT IN (SELECT id FROM _sym_dict)"
            ),
        );
        assert_eq!(
            orphan, 0,
            "{rel}.{col} holds {orphan} cell(s) that are not dense _sym_dict ids (hash leak)"
        );
    }

    // 4. Join parity: the dense cross-family join equals the text join.
    let dense_join = count(
        &eng,
        "SELECT COUNT(*) FROM rel_call_edge e JOIN rel_call_def d ON e.caller = d.sym",
    );
    let text_join = count(
        &eng,
        "SELECT COUNT(*) FROM rel_call_edge_txt e JOIN rel_call_def_txt d ON e.caller = d.sym",
    );
    assert_eq!(
        dense_join, text_join,
        "join parity broken: caller<->call_def.sym dense join {dense_join} vs text join {text_join}"
    );
    assert!(
        dense_join > 0,
        "call_edge.caller <-> call_def.sym join is empty — the gate would be vacuous"
    );

    // 5. SQL write-path parity: a SQL ground fact (`word`, lowered through
    //    `sprf_sym_intern`) and a SQL derived rel (`echo_word`, pass-through)
    //    must store the SAME dense id for the SAME symbol, so their integer join
    //    equals both the fact-row count and the decoded-text join. NOTE (codex/
    //    opus round-2 correction): both sides lower through the SQL path, so this
    //    does NOT by itself catch the 624ba534 split (under it both would have
    //    been raw-hash and still agreed). The split is caught by the atomicity
    //    check (case 3: no cell outside `_sym_dict`) and the router-written
    //    call/type cross-family join (case 4). This case pins that
    //    `sprf_sym_intern` agrees with itself across fact and derived lowering.
    let word_rows = count(&eng, "SELECT COUNT(*) FROM rel_word");
    assert!(word_rows >= 2, "fixture: word should have 2 fact rows, got {word_rows}");
    let dense_word_join = count(
        &eng,
        "SELECT COUNT(*) FROM rel_word w JOIN rel_echo_word e ON w.w = e.w",
    );
    let text_word_join = count(
        &eng,
        "SELECT COUNT(*) FROM rel_word_txt w JOIN rel_echo_word_txt e ON w.w = e.w",
    );
    assert_eq!(
        dense_word_join, word_rows,
        "SQL fact `word` and SQL derived `echo_word` disagree on the dense id for a \
         symbol: dense self-join {dense_word_join} != {word_rows} fact rows"
    );
    assert_eq!(
        dense_word_join, text_word_join,
        "cross-write-path join parity broken: dense {dense_word_join} vs decoded {text_word_join}"
    );

    let _ = fs::remove_dir_all(&dir);
}
