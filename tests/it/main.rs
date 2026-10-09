mod _0_read_lower_check;
mod _10_query;
mod _11_watchers;
mod _1_compile_oracles;
mod _2_eval_oracles;
mod _3_eval_fixtures;
mod _4_sqlite;
mod _5_store_host;
mod _6_imports;
mod _7_book;
mod _8_contracts;
mod _9_ladder;
mod _9a_ir_emit;
mod _9b_sum_variants;
mod _9c_generic_bind;
mod _9d_string_kernel;
mod _9e_generic_rules;
mod _9f_extends;
mod _9g_call_hand;
mod _9h_build;
mod _9i_kernel_names;
mod _9j_no_recursion;
mod _9k_next;
mod _9l_macrotime_waves;
mod _9l_rule_counts;
mod _9m_one_engine;
mod _9n_memory_ladder;

/// `DL8_RULES` as the binary reads it: unset is `compile`. Work-count pins differ by mode.
fn rules_interpreted() -> bool {
    std::env::var("DL8_RULES").as_deref() == Ok("interpret")
}
