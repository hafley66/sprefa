// The NEGATIVE control: a source and a sink in one file with no path joining them.

fn read_request_body() -> String {
    String::from("drop table users")
}

fn execute_sql(statement_text: String) -> String {
    statement_text
}

fn handle_unrelated_request() -> String {
    let untrusted_payload = read_request_body();
    let constant_statement = String::from("select 1");
    execute_sql(constant_statement)
}
