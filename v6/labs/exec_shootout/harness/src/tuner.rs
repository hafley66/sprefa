use crate::gen::{self, Family, Params};
use crate::refengine::{derive_count_bitset, ref_result};

pub const BAND_MIN: u64 = gen::BAND_MIN_DERIVED;
pub const BAND_MAX: u64 = gen::BAND_MAX_DERIVED;
const MIDPOINT: u64 = 10_000_000;
const MAX_LAYERED_NODES: u32 = 60_000;

pub struct Tuned {
    pub params: Params,
    pub derived: u64,
    pub edge_count: u64,
}

fn chain_derived(segment_len: u64, scale: u64) -> u64 {
    let edges_per_segment = segment_len - 1;
    let segments = (scale / edges_per_segment).max(1);
    segments * segment_len * edges_per_segment / 2
}

fn grid_derived(rows: u64, cols: u64) -> u64 {
    let row_pairs = rows * (rows + 1) / 2;
    let col_pairs = cols * (cols + 1) / 2;
    row_pairs * col_pairs - rows * cols
}

fn grid_edges(rows: u64, cols: u64) -> u64 {
    rows * (cols - 1) + cols * (rows - 1)
}

fn chain_edges(segment_len: u64, scale: u64) -> u64 {
    let edges_per_segment = segment_len - 1;
    let segments = (scale / edges_per_segment).max(1);
    segments * edges_per_segment
}

fn largest_leq(mut low: u64, mut high: u64, mut value: impl FnMut(u64) -> u64, limit: u64) -> u64 {
    while low < high {
        let mid = (low + high + 1) / 2;
        if value(mid) <= limit {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    low
}

fn tune_chain(scale: u32) -> Tuned {
    let scale_64 = scale as u64;
    let high = scale_64.max(2);
    let upper = largest_leq(
        2,
        high,
        |segment_len| chain_derived(segment_len, scale_64),
        BAND_MAX,
    );
    let lower = largest_leq(
        2,
        upper,
        |segment_len| chain_derived(segment_len, scale_64),
        BAND_MIN - 1,
    ) + 1;
    let mut chosen = lower;
    let mut best_distance = u64::MAX;
    for candidate in lower..=upper {
        let derived = chain_derived(candidate, scale_64);
        let distance = derived.abs_diff(MIDPOINT);
        if distance < best_distance {
            best_distance = distance;
            chosen = candidate;
        }
    }
    Tuned {
        params: Params::Chain {
            segment_len: chosen as u32,
        },
        derived: chain_derived(chosen, scale_64),
        edge_count: chain_edges(chosen, scale_64),
    }
}

fn grid_target_derived(scale: u32) -> u64 {
    let scale_log = (scale as f64).log10();
    let lower_log = 10_000f64.log10();
    let upper_log = 1_000_000f64.log10();
    let fraction = ((scale_log - lower_log) / (upper_log - lower_log)).clamp(0.0, 1.0);
    let low_log = (BAND_MIN as f64).log10();
    let high_log = (BAND_MAX as f64).log10();
    let result_log = low_log + fraction * (high_log - low_log);
    10f64.powf(result_log).round() as u64
}

fn tune_grid(scale: u32) -> Tuned {
    let target = grid_target_derived(scale);
    let upper = largest_leq(2, 10_000, |side| grid_derived(side, side), BAND_MAX);
    let lower = largest_leq(2, upper, |side| grid_derived(side, side), BAND_MIN - 1) + 1;
    let mut chosen = lower;
    let mut best_distance = u64::MAX;
    for side in lower..=upper {
        let derived = grid_derived(side, side);
        let distance = derived.abs_diff(target);
        if distance < best_distance {
            best_distance = distance;
            chosen = side;
        }
    }
    Tuned {
        params: Params::Grid {
            rows: chosen as u32,
            cols: chosen as u32,
        },
        derived: grid_derived(chosen, chosen),
        edge_count: grid_edges(chosen, chosen),
    }
}

fn layered_width(layers: u64, fanout: u64, scale: u64) -> u64 {
    let max_width = MAX_LAYERED_NODES as u64 / layers;
    let budget = scale / ((layers - 1) * fanout);
    budget.min(max_width).max(1)
}

fn measure_layered(layers: u64, fanout: u64, scale: u64, seed: u64) -> (u64, u64) {
    let width = layered_width(layers, fanout, scale);
    let params = Params::Layered {
        layers: layers as u32,
        width: width as u32,
        fanout: fanout as u32,
    };
    let generated = gen::generate(Family::Layered, params, scale as u32, seed);
    let node_count = generated.node_count as u64;
    let derived = if node_count <= MAX_LAYERED_NODES as u64 {
        derive_count_bitset(&generated.edges, generated.node_count)
    } else {
        ref_result(&generated.edges).derived
    };
    (derived, generated.edges.len() as u64)
}

const FANOUT_TUNES: [u64; 4] = [2, 4, 8, 16];

fn tune_layered(scale: u32, seed: u64) -> Tuned {
    let scale_64 = scale as u64;
    let mut best: Option<Tuned> = None;
    let mut best_distance = u64::MAX;
    for fanout in FANOUT_TUNES {
        let upper = largest_leq(
            2,
            200,
            |layers| measure_layered(layers, fanout, scale_64, seed).0,
            BAND_MAX,
        );
        let lower = largest_leq(
            2,
            upper,
            |layers| measure_layered(layers, fanout, scale_64, seed).0,
            BAND_MIN - 1,
        ) + 1;
        for layers in lower..=upper {
            let (derived, edge_count) = measure_layered(layers, fanout, scale_64, seed);
            if derived >= BAND_MIN && derived <= BAND_MAX {
                let distance = derived.abs_diff(MIDPOINT);
                if distance < best_distance {
                    best_distance = distance;
                    let width = layered_width(layers, fanout, scale_64);
                    best = Some(Tuned {
                        params: Params::Layered {
                            layers: layers as u32,
                            width: width as u32,
                            fanout: fanout as u32,
                        },
                        derived,
                        edge_count,
                    });
                }
            }
        }
    }
    match best {
        Some(found) => found,
        None => panic!("layered tuner found no in-band case at scale {}", scale),
    }
}

pub fn tune(family: Family, scale: u32) -> Tuned {
    let seed = scale as u64 ^ 0x5eed_cafe;
    match family {
        Family::Chain => tune_chain(scale),
        Family::Layered => tune_layered(scale, seed),
        Family::Grid => tune_grid(scale),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid_key(params: Params) -> (u32, u32) {
        match params {
            Params::Grid { rows, cols } => (rows, cols),
            other => panic!("not a grid: {:?}", other),
        }
    }

    #[test]
    fn grid_tuner_is_scale_aware_and_in_band() {
        let scales = [10_000u32, 100_000, 1_000_000];
        let tuned: Vec<Tuned> = scales
            .iter()
            .map(|scale| tune(Family::Grid, *scale))
            .collect();
        let keys: Vec<(u32, u32)> = tuned.iter().map(|entry| grid_key(entry.params)).collect();
        assert!(keys[0] != keys[1]);
        assert!(keys[1] != keys[2]);
        assert!(keys[0] != keys[2]);
        for entry in &tuned {
            assert!(
                entry.derived >= BAND_MIN && entry.derived <= BAND_MAX,
                "params {:?} derived {} out of band",
                entry.params,
                entry.derived
            );
        }
    }
}
