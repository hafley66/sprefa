// The SANITIZED arm: the same source and sink shape with escape_sql on the path.

fn read_request_body() -> String {
    String::from("drop table users")
}

fn escape_sql(raw_text: String) -> String {
    raw_text
}

fn execute_sql(statement_text: String) -> String {
    statement_text
}

fn run_database_query(query_text: String) -> String {
    execute_sql(query_text)
}

fn handle_safe_request() -> String {
    let untrusted_payload = read_request_body();
    let sanitized_value = escape_sql(untrusted_payload);
    run_database_query(sanitized_value)
}
