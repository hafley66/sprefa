// The POSITIVE arm: one source, one sink, one function boundary between them.

fn read_request_body() -> String {
    String::from("drop table users")
}

fn execute_sql(statement_text: String) -> String {
    statement_text
}

fn run_database_query(query_text: String) -> String {
    execute_sql(query_text)
}

fn handle_request() -> String {
    let untrusted_payload = read_request_body();
    run_database_query(untrusted_payload)
}
