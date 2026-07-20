#[path = "14_bootstrap.rs"]
mod bootstrap;
#[path = "8_check.rs"]
mod check;
#[path = "15_cli.rs"]
mod cli;
#[path = "13_codegen_js.rs"]
mod codegen_js;
#[path = "12_codegen_rust.rs"]
mod codegen_rust;
#[path = "9_eval.rs"]
mod eval;
#[path = "10_facts.rs"]
mod facts;
#[path = "0_ids.rs"]
mod ids;
#[path = "4_parser.rs"]
mod parser;
#[path = "6_patterns.rs"]
mod patterns;
#[path = "11_rules.rs"]
mod rules;
#[path = "2_source.rs"]
mod source;
#[path = "7_store.rs"]
mod store;
#[path = "1_symbols.rs"]
mod symbols;
#[path = "3_syntax.rs"]
mod syntax;
#[path = "5_types.rs"]
mod types;

pub use ids::*;
pub use parser::ParseOutput;
pub use patterns::*;
pub use source::Source;
pub use symbols::SymbolTable;
pub use types::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let command = args.next().unwrap_or_else(|| "generate".to_owned());
    let input = args
        .next()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("schema.dl"));
    let output = cli::default_output_dir();
    let store = cli::compile(&input).map_err(std::io::Error::other)?;
    match command.as_str() {
        "check" => {
            check::check(&store).map_err(|errors| std::io::Error::other(errors.join("\n")))?;
            println!("{}", store.dump());
        }
        "generate" => {
            cli::generate(&store, &output).map_err(std::io::Error::other)?;
            println!("generated semantic artifacts in {}", output.display());
        }
        "bootstrap" => {
            let report = bootstrap::bootstrap(&store, &output).map_err(std::io::Error::other)?;
            print!("{report}");
        }
        other => {
            return Err(std::io::Error::other(format!(
                "unknown command {other}; expected check, generate, or bootstrap"
            ))
            .into())
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn store(source: &str) -> store::Store {
        let source = Source {
            id: SourceId(0),
            text: source.to_owned(),
        };
        store::Store::new(source.clone())
            .lower(parser::parse(&source))
            .unwrap()
    }

    #[test]
    fn mixed_slots_preserve_spelling_and_normalize() {
        let store =
            store("type UserId = String\npattern UserEvent = `users/:id/events/{kind: UserId}`\n");
        let id = match store.declarations[&store.symbols.get("UserEvent").unwrap()] {
            store::Declaration::Pattern(id) => id,
            _ => unreachable!(),
        };
        assert_eq!(store.pattern_text(id), "users/:id/events/{kind: UserId}");
        let slots = eval::enumerate_slots(&store, id)
            .map(|slot| (slot.source.clone(), slot.spelling.as_str(), slot.position))
            .collect::<Vec<_>>();
        assert_eq!(
            slots,
            vec![
                ("id".to_owned(), "colon", 0),
                ("kind".to_owned(), "braces", 1)
            ]
        );
    }

    #[test]
    fn bind_match_roundtrip_and_errors_are_deterministic() {
        let store = store("pattern P = `users/{id: Int}/events/:kind`\n");
        let id = match store.declarations[&store.symbols.get("P").unwrap()] {
            store::Declaration::Pattern(id) => id,
            _ => unreachable!(),
        };
        let input = eval::bind(
            &store,
            id,
            &[
                ArgumentValue::Named("id".to_owned(), Value::Int(42)),
                ArgumentValue::Positional(Value::String("created".to_owned())),
            ],
        )
        .unwrap();
        assert_eq!(input, "users/42/events/created");
        let bindings = eval::match_pattern(&store, id, &input).unwrap();
        assert_eq!(
            bindings.positional,
            vec![Value::Int(42), Value::String("created".to_owned())]
        );
        assert_eq!(
            eval::bind(
                &store,
                id,
                &[ArgumentValue::Named("id".to_owned(), Value::Int(42))]
            ),
            Err(PatternError::MissingBinding("kind".to_owned()))
        );
    }

    #[test]
    fn destructure_and_compose_retain_typed_slots() {
        let mut store = store("pattern Base = `users/{id: Int}`\npattern Tail = `/events/:kind`\n");
        let base = match store.declarations[&store.symbols.get("Base").unwrap()] {
            store::Declaration::Pattern(id) => id,
            _ => unreachable!(),
        };
        let tail = match store.declarations[&store.symbols.get("Tail").unwrap()] {
            store::Declaration::Pattern(id) => id,
            _ => unreachable!(),
        };
        let composed = eval::compose(&mut store, base, tail).unwrap();
        assert_eq!(store.pattern_text(composed), "users/{id: Int}/events/:kind");
        assert_eq!(
            eval::destructure(&store, composed, "users/7/events/created")
                .unwrap()
                .positional,
            vec![Value::Int(7), Value::String("created".to_owned())]
        );
    }

    #[test]
    fn path_enumeration_covers_nested_shapes() {
        let store = store("type Profile = { name: String }\ntype User = { profile: Optional<Profile>, tags: Array<String>, metadata: Map<String, String> }\n");
        let root = store.lookup_type("User").unwrap();
        let paths = eval::enumerate_paths(&store, root)
            .into_iter()
            .map(|path| path.text)
            .collect::<Vec<_>>();
        assert_eq!(
            paths,
            vec![
                "profile",
                "profile.name",
                "tags",
                "tags[*]",
                "metadata",
                "metadata{key}"
            ]
        );
    }

    #[test]
    fn recursive_path_enumeration_stops_at_the_active_type() {
        let store = store("type Node { value: String next: Node }\n");
        let root = store.lookup_type("Node").unwrap();
        let paths = eval::enumerate_paths(&store, root)
            .into_iter()
            .map(|path| path.text)
            .collect::<Vec<_>>();
        assert_eq!(paths, vec!["value", "next"]);
    }

    #[test]
    fn generated_rust_and_js_are_full_strings() {
        let store = store("type User = { id: String }\npattern UserPath = `/users/{id}`\nconsumer http { get UserPath -> User }\n");
        let rust = codegen_rust::emit_models(&store);
        assert_eq!(rust, "// generated by bootstrap-typegen-lab\n\n#[derive(Debug, Clone)]\npub struct User {\n    pub id: String,\n}\n\n");
        let js = codegen_js::emit_client(&store);
        assert_eq!(js, "// generated by bootstrap-typegen-lab\n\nconst baseUrl = globalThis.BASE_URL ?? \"http://127.0.0.1:4000\";\n\n/** @returns {Promise<User>} */\nexport async function get(id) {\n  const response = await fetch(baseUrl + `/users/${encodeURIComponent(id)}`);\n  if (!response.ok) throw new Error(\"HTTP \" + response.status);\n  return await response.json();\n}\n\n");
    }

    #[test]
    fn facts_emit_record_fields() {
        let store = store("type User { id: String active: Bool }\n");
        let root = store.lookup_type("User").unwrap();
        let mut facts = facts::FactStore::default();
        rules::saturate(&store, &mut facts);
        let fields = facts
            .facts
            .iter()
            .filter_map(|fact| match fact {
                facts::Fact::Field(owner, name, _) if *owner == root => {
                    Some(store.symbols.resolve(*name).to_owned())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(fields, vec!["id", "active"]);
    }

    #[test]
    fn generated_server_has_typed_matchers_and_valid_404_json() {
        let store = store("type EventKind = \"created\" | \"deleted\"\ntype Response = { ok: Bool }\npattern CountPath = `/items/{id: Int}`\npattern FlagPath = `/flags/{enabled: Bool}`\npattern EventPath = `/events/{kind: EventKind}`\nconsumer http { get CountPath -> Response }\nconsumer http { get FlagPath -> Response }\nconsumer http { get EventPath -> Response }\n");
        let output = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/typed-matcher-generated");
        cli::generate(&store, &output).unwrap();
        let server = std::fs::read_to_string(output.join("server.rs")).unwrap();
        assert!(server.contains("template_matches(\"/items/{id}\", &[SlotKind::Int], path)"));
        assert!(server.contains("template_matches(\"/flags/{enabled}\", &[SlotKind::Bool], path)"));
        assert!(server.contains("template_matches(\"/events/{kind}\", &[SlotKind::OneOf(&[\"created\", \"deleted\"])], path)"));
        assert!(server.contains(r##"r#"{"error":"not found"}"#"##));
        assert!(server.contains(
            "assert!(!template_matches(\"/items/{id}\", &[SlotKind::Int], \"/items/invalid\"));"
        ));
    }

    #[test]
    fn malformed_input_and_duplicate_bindings_report_text() {
        let source = Source {
            id: SourceId(0),
            text: "pattern Bad = `users/{id`\npattern Bad = `x/:id`\n".to_owned(),
        };
        let parsed = parser::parse(&source);
        assert_eq!(parsed.module.declarations.len(), 1);
        assert_eq!(parsed.diagnostics, vec!["0:24: expected '}'"]);
        let source = Source {
            id: SourceId(0),
            text: "pattern P = `/{id}/{id}`\n".to_owned(),
        };
        assert!(
            matches!(store::Store::new(source.clone()).lower(parser::parse(&source)), Err(errors) if errors == vec!["duplicate binding id".to_owned()])
        );
    }

    #[test]
    fn generated_rust_is_compilable_and_bootstrap_boundary_is_explicit() {
        let store = store("type User = { id: String }\npattern UserPath = `/users/{id}`\nconsumer http { get UserPath -> User }\n");
        let output = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-generated");
        cli::generate(&store, &output).unwrap();
        let report = bootstrap::bootstrap(&store, &output).unwrap();
        assert!(report.contains("stage-one self-regeneration stops at the parser/emitter boundary"));
        assert!(std::fs::read_to_string(output.join("server.rs"))
            .unwrap()
            .contains("template_matches"));
    }
}
