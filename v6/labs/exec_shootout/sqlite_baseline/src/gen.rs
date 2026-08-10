use std::collections::HashSet;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Family {
    Chain,
    Layered,
    Grid,
}

#[derive(Clone, Copy)]
pub enum Params {
    Chain {
        segment_len: u32,
    },
    Layered {
        layers: u32,
        width: u32,
        fanout: u32,
    },
    Grid {
        rows: u32,
        cols: u32,
    },
}

pub struct Generated {
    pub edge_count: u32,
    pub node_count: u32,
    pub edges: Vec<(u32, u32)>,
}

pub fn generate(family: Family, params: Params, scale: u32, seed: u64) -> Generated {
    match params {
        Params::Chain { segment_len } => generate_chain(segment_len, scale),
        Params::Layered {
            layers,
            width,
            fanout,
        } => generate_layered(layers, width, fanout, seed),
        Params::Grid { rows, cols } => generate_grid(rows, cols),
    }
}

fn generate_chain(segment_len: u32, scale: u32) -> Generated {
    let edges_per_segment = segment_len - 1;
    let segments = (scale / edges_per_segment).max(1);
    let mut edges = Vec::with_capacity((segments * edges_per_segment) as usize);
    let mut cursor: u32 = 0;
    for _ in 0..segments {
        for offset in 0..segment_len - 1 {
            edges.push((cursor + offset, cursor + offset + 1));
        }
        cursor += segment_len;
    }
    let node_count = cursor;
    Generated {
        edge_count: edges.len() as u32,
        node_count,
        edges,
    }
}

fn generate_layered(layers: u32, width: u32, fanout: u32, seed: u64) -> Generated {
    let mut rng = Prng::new(seed);
    let mut edges = Vec::new();
    for layer in 0..layers - 1 {
        let next_layer_start = (layer + 1) * width;
        for index_in_layer in 0..width {
            let source = layer * width + index_in_layer;
            let mut chosen: HashSet<u32> = HashSet::default();
            for _ in 0..fanout {
                if chosen.len() as u32 >= width {
                    break;
                }
                let candidate = loop {
                    let probe = next_layer_start + rng.next_below(width);
                    if !chosen.contains(&probe) {
                        break probe;
                    }
                };
                chosen.insert(candidate);
                edges.push((source, candidate));
            }
        }
    }
    Generated {
        edge_count: edges.len() as u32,
        node_count: layers * width,
        edges,
    }
}

fn generate_grid(rows: u32, cols: u32) -> Generated {
    let mut edges = Vec::new();
    for row in 0..rows {
        for col in 0..cols {
            let node = row * cols + col;
            if col + 1 < cols {
                edges.push((node, node + 1));
            }
            if row + 1 < rows {
                edges.push((node, node + cols));
            }
        }
    }
    Generated {
        edge_count: edges.len() as u32,
        node_count: rows * cols,
        edges,
    }
}

struct Prng {
    state: u64,
}

impl Prng {
    fn new(seed: u64) -> Self {
        Prng { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E3779B97F4A7C15);
        let mut value = self.state;
        value = (value ^ (value >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94D049BB133111EB);
        value ^ (value >> 31)
    }

    fn next_below(&mut self, bound: u32) -> u32 {
        if bound == 0 {
            return 0;
        }
        (self.next_u64() % bound as u64) as u32
    }
}

pub fn seed_for_scale(scale: u32) -> u64 {
    scale as u64 ^ 0x5eed_cafe
}
