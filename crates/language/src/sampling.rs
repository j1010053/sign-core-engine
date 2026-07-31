//! Stateless, deterministic weighted sampling shared by synchronous and
//! diachronic authoring paths.

use rand_chacha::ChaCha20Rng;
use rand_core::{RngCore, SeedableRng};

pub const WEIGHTED_SAMPLER_ALGORITHM: &str = "rand_chacha/ChaCha20Rng@0.3";

#[derive(Debug, Clone, PartialEq)]
pub struct WeightedSampleTrace {
    pub algorithm: &'static str,
    pub seed: u64,
    pub normalized_weights: Vec<f64>,
    pub selected_index: usize,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum WeightedSampleError {
    #[error("weighted sampling requires at least one candidate")]
    Empty,
    #[error("weight at index {index} must be finite and non-negative, got {weight}")]
    InvalidWeight { index: usize, weight: f64 },
    #[error("all candidate weights are zero")]
    AllZero,
}

/// Select one index from ordered non-negative weights.
///
/// Weights are divided by their maximum before summing, avoiding overflow
/// without changing ratios. ChaCha20 is seeded explicitly rather than using
/// environment randomness, so the same ordered weights and seed are replayable.
pub fn sample_weighted_index(
    weights: &[f64],
    seed: u64,
) -> Result<WeightedSampleTrace, WeightedSampleError> {
    if weights.is_empty() {
        return Err(WeightedSampleError::Empty);
    }
    for (index, weight) in weights.iter().copied().enumerate() {
        if !weight.is_finite() || weight < 0.0 {
            return Err(WeightedSampleError::InvalidWeight { index, weight });
        }
    }
    let max_weight = weights.iter().copied().fold(0.0_f64, f64::max);
    if max_weight == 0.0 {
        return Err(WeightedSampleError::AllZero);
    }
    let normalized_weights = weights
        .iter()
        .map(|weight| weight / max_weight)
        .collect::<Vec<_>>();
    let total = normalized_weights.iter().sum::<f64>();

    let mut random = ChaCha20Rng::seed_from_u64(seed);
    let unit = (random.next_u64() >> 11) as f64 / ((1u64 << 53) as f64);
    let mut draw = unit * total;
    let selected_index = normalized_weights
        .iter()
        .enumerate()
        .filter(|(_, weight)| **weight > 0.0)
        .find_map(|(index, weight)| {
            if draw < *weight {
                Some(index)
            } else {
                draw -= *weight;
                None
            }
        })
        .or_else(|| normalized_weights.iter().rposition(|weight| *weight > 0.0))
        .ok_or(WeightedSampleError::AllZero)?;

    Ok(WeightedSampleTrace {
        algorithm: WEIGHTED_SAMPLER_ALGORITHM,
        seed,
        normalized_weights,
        selected_index,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_and_distribution_are_reproducible() {
        let first = sample_weighted_index(&[1.0, 3.0], 42).unwrap();
        let second = sample_weighted_index(&[1.0, 3.0], 42).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn normalization_avoids_finite_sum_overflow() {
        let trace = sample_weighted_index(&[1.7e308, 8.5e307], 3).unwrap();
        assert_eq!(trace.normalized_weights, [1.0, 0.5]);
        assert_eq!(trace.selected_index, 0);
    }

    #[test]
    fn chacha20_seed_sequence_is_versioned() {
        let selected = (0..16)
            .map(|seed| {
                sample_weighted_index(&[1.0, 0.5], seed)
                    .unwrap()
                    .selected_index
            })
            .collect::<Vec<_>>();
        assert_eq!(selected, [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 1]);
    }

    #[test]
    fn invalid_and_zero_distributions_are_rejected() {
        assert_eq!(
            sample_weighted_index(&[], 0),
            Err(WeightedSampleError::Empty)
        );
        assert_eq!(
            sample_weighted_index(&[0.0, 0.0], 0),
            Err(WeightedSampleError::AllZero)
        );
        assert!(matches!(
            sample_weighted_index(&[1.0, f64::NAN], 0),
            Err(WeightedSampleError::InvalidWeight { index: 1, .. })
        ));
        assert!(matches!(
            sample_weighted_index(&[-1.0], 0),
            Err(WeightedSampleError::InvalidWeight { index: 0, .. })
        ));
    }

    #[test]
    fn many_seeds_follow_the_weight_ratio_without_flakiness() {
        let selected_second = (0..10_000)
            .filter(|seed| {
                sample_weighted_index(&[1.0, 3.0], *seed)
                    .unwrap()
                    .selected_index
                    == 1
            })
            .count();
        assert!(
            (7_300..=7_700).contains(&selected_second),
            "expected a stable 3:1 distribution, got {selected_second}/10000"
        );
    }
}
