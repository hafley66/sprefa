//! `Recipe::parse` over written texts: canonical form or the diag.

use v9::_4_recipe::Recipe;

#[test]
fn parse_table() {
    let texts = [
        "",
        "engine=dd,store=sqlite,wire=http",
        " Store = SQLITE ; engine=dd ",
        "wire=http,",
        "engine=dd;;store=sqlite",
        "engine=timely",
        "store=postgres",
        "wire=grpc",
        "cache=lru",
        "engine=dd,engine=dd",
        "engine",
    ];
    let out: Vec<_> = texts
        .iter()
        .map(|text| (*text, Recipe::parse(text).map(|recipe| recipe.canonical())))
        .collect();
    insta::assert_debug_snapshot!(out);
}

#[test]
fn canonical_round_trips() {
    let recipe = Recipe::parse("wire=http;store=sqlite").expect("parses");
    assert_eq!(Recipe::parse(&recipe.canonical()), Ok(recipe));
    assert_eq!(recipe, Recipe::default());
}
