// The CFL arm: one helper reached from two call sites, only one of them tainted.
// A context-insensitive return edge leaves the callee at the WRONG site and
// carries taint into the clean value; the site-indexed walk refuses that hop.

fn read_request_body() -> String {
    String::from("drop table users")
}

fn execute_sql(statement_text: String) -> String {
    statement_text
}

fn identity_passthrough(passed_value: String) -> String {
    passed_value
}

fn handle_two_site_request() -> String {
    let untrusted_payload = read_request_body();
    let discarded_echo = identity_passthrough(untrusted_payload);
    let constant_statement = String::from("select 1");
    let clean_echo = identity_passthrough(constant_statement);
    execute_sql(clean_echo)
}
