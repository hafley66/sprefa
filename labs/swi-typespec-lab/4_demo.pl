:- use_module(library(plunit)).
:- use_module('0_schema').
:- use_module('1_types').
:- use_module('2_patterns').
:- use_module('3_codegen').
:- use_module('5_documents').

:- begin_tests(swi_typespec_lab).

test(surface_schema_parses_to_semantic_terms) :-
    Source = "type Box { value: String; tags: String[]; } pattern BoxPath = `/boxes/:id`; consumer http { get BoxPath -> Box; }",
    parse_schema(Source, Declarations),
    assertion(Declarations == [
        type_decl(box, model([field(value, string), field(tags, array(string))])),
        pattern_decl(box_path, "/boxes/:id"),
        consumer(http, get, box_path, box)
    ]).

test(pattern_source_roundtrip) :-
    once(parse_pattern("/users/{id: UserId}", Parts)),
    assertion(Parts == [literal("/users/"), slot(id, user_id)]),
    once(pattern_string(Parts, Source)),
    assertion(Source == "/users/{id:user_id}").

test(pattern_match_and_render_same_relation) :-
    once(parse_pattern("users/:id/events/{kind: EventKind}", Parts)),
    once(pattern_value(Parts, Bindings, "users/alice/events/created")),
    assertion(Bindings == [id-"alice", kind-"created"]),
    once(pattern_value(Parts, [id-"bob", kind-"deleted"], Rendered)),
    assertion(Rendered == "users/bob/events/deleted").

test(pattern_rejects_union_member, [fail]) :-
    parse_pattern("users/:id/events/{kind: EventKind}", Parts),
    pattern_value(Parts, _, "users/alice/events/renamed").

test(json_type_acceptance) :-
    User = user{id:"abc", profile:profile{name:"Ada"}, tags:["admin"], metadata:_{source:"test"}},
    once(accepts(user, User)).

test(json_type_rejection, [fail]) :-
    accepts(user, user{id:7, tags:[], metadata:_{}}).

test(path_enumeration) :-
    setof(Path, type_path(user, Path), Paths),
    assertion(Paths == ["id", "metadata{key}", "profile.name", "tags[*]"]).

test(rust_codegen) :-
    rust_source(Source),
    once(sub_string(Source, _, _, _, "pub struct User")),
    once(sub_string(Source, _, _, _, "pub metadata: std::collections::BTreeMap<String, String>")).

test(javascript_codegen) :-
    javascript_source(Source),
    once(sub_string(Source, _, _, _, "fetch(baseUrl + `/users/${encodeURIComponent(id)}`")).

test(lsp_semantic_diagnostic_and_utf16_position) :-
    Text = "// 😀\ntype UserId = String;\ntype User { id: UserId; missing: Missing; }",
    open_document("file:///test.soup", 1, Text, _),
    document_diagnostics("file:///test.soup", Diagnostics),
    assertion(Diagnostics = [_{message:"Undefined type missing", range:_, severity:1, source:"soup"}]),
    hover_at("file:///test.soup", 2, 16, Hover),
    assertion(Hover.contents.value == "```soup\ntype user_id = alias(string)\n```"),
    close_document("file:///test.soup").

test(lsp_parse_diagnostic) :-
    open_document("file:///broken.soup", 1, "type Broken {", _),
    document_diagnostics("file:///broken.soup", Diagnostics),
    assertion(Diagnostics = [_{message:"Soup parser could not recover at this position", range:_, severity:1, source:"soup"}]),
    close_document("file:///broken.soup").

:- end_tests(swi_typespec_lab).

main :-
    run_tests,
    rust_source(Rust),
    javascript_source(JavaScript),
    make_directory_path('generated'),
    setup_call_cleanup(open('generated/models.rs', write, RustStream), write(RustStream, Rust), close(RustStream)),
    setup_call_cleanup(open('generated/client.mjs', write, JsStream), write(JsStream, JavaScript), close(JsStream)),
    parse_pattern("users/:id/events/{kind: EventKind}", Parts),
    pattern_value(Parts, Bindings, "users/alice/events/created"),
    pattern_value(Parts, [id-"bob", kind-"deleted"], Rendered),
    setof(Path, type_path(user, Path), Paths),
    format("matched: ~q~nrendered: ~s~npaths: ~q~n", [Bindings, Rendered, Paths]).

:- initialization(main, main).
