//! Kuzu: embeddable property graph, queried in Cypher (Neo4j's language),
//! with columnar / worst-case-optimal-join execution. Same call graph; the
//! transitive reach is a Cypher variable-length path `-[:Calls*]->`.

use std::collections::BTreeSet;
use kuzu::{Connection, Database, SystemConfig};
use v5cozokuzu::EDGES;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::temp_dir().join("v5kuzu_db");
    let _ = std::fs::remove_dir_all(&dir);

    let db = Database::new(&dir, SystemConfig::default())?;
    let conn = Connection::new(&db)?;

    // schema: a node table and a relationship table
    conn.query("CREATE NODE TABLE Func(name STRING, PRIMARY KEY(name))")?;
    conn.query("CREATE REL TABLE Calls(FROM Func TO Func)")?;

    // nodes (every endpoint that appears in an edge)
    let mut names: BTreeSet<&str> = BTreeSet::new();
    for (a, b) in EDGES { names.insert(a); names.insert(b); }
    for n in &names {
        conn.query(&format!("CREATE (:Func {{name: '{n}'}})"))?;
    }
    // edges
    for (a, b) in EDGES {
        conn.query(&format!(
            "MATCH (x:Func {{name: '{a}'}}), (y:Func {{name: '{b}'}}) CREATE (x)-[:Calls]->(y)"
        ))?;
    }

    // transitive reach from main: Cypher variable-length path
    let result = conn.query(
        "MATCH (:Func {name: 'main'})-[:Calls*]->(b:Func) RETURN DISTINCT b.name"
    )?;
    println!("reach from main (Cypher variable-length path):");
    for row in result {
        let cells: Vec<String> = row.iter().map(|v| format!("{v}")).collect();
        println!("  {}", cells.join("\t"));
    }
    Ok(())
}
