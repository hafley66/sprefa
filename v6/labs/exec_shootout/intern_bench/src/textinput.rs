use crate::keys::TEXT_HEADER_TAG;

// The .tin format: `p <nodes> <edges> text4`, then one tab separated
// (from_path, from_name, to_path, to_name) row per line.
pub struct TextHeader {
    pub node_count: u32,
    pub edge_count: u32,
}

pub fn parse_header(line: &str) -> Result<TextHeader, String> {
    let mut fields = line.split_whitespace();
    if fields.next() != Some("p") {
        return Err("first token must be 'p'".to_string());
    }
    let node_count = fields
        .next()
        .and_then(|field| field.parse::<u32>().ok())
        .ok_or_else(|| "missing node count in header".to_string())?;
    let edge_count = fields
        .next()
        .and_then(|field| field.parse::<u32>().ok())
        .ok_or_else(|| "missing edge count in header".to_string())?;
    if fields.next() != Some(TEXT_HEADER_TAG) {
        return Err(format!("header must end with '{TEXT_HEADER_TAG}'"));
    }
    Ok(TextHeader {
        node_count,
        edge_count,
    })
}

pub struct TextEdge<'line> {
    pub from_path: &'line str,
    pub from_name: &'line str,
    pub to_path: &'line str,
    pub to_name: &'line str,
}

pub fn parse_edge(line: &str) -> Result<TextEdge<'_>, String> {
    let mut fields = line.split('\t');
    let from_path = fields.next().ok_or_else(|| "missing from_path".to_string())?;
    let from_name = fields.next().ok_or_else(|| "missing from_name".to_string())?;
    let to_path = fields.next().ok_or_else(|| "missing to_path".to_string())?;
    let to_name = fields.next().ok_or_else(|| "missing to_name".to_string())?;
    if fields.next().is_some() {
        return Err("edge line has more than four columns".to_string());
    }
    Ok(TextEdge {
        from_path,
        from_name,
        to_path,
        to_name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_and_edge_parse() {
        let header = parse_header("p 100 200 text4").expect("header");
        assert_eq!(header.node_count, 100);
        assert_eq!(header.edge_count, 200);
        let edge = parse_edge("a/b.ts\tone\tc/d.ts\ttwo").expect("edge");
        assert_eq!(edge.from_path, "a/b.ts");
        assert_eq!(edge.to_name, "two");
    }

    #[test]
    fn an_int_keyed_header_is_refused() {
        assert!(parse_header("p 100 200").is_err());
    }
}
