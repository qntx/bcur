//! Walker's alias method sampler (ordered partition matching `ur-rs`).

use alloc::vec::Vec;

use super::Xoshiro256;

/// Weighted discrete sampler over `0..weights.len()`.
#[derive(Debug)]
pub(crate) struct Weighted {
    aliases: Vec<u32>,
    probs: Vec<f64>,
}

impl Weighted {
    /// Builds a sampler from non-negative weights that sum to a positive value.
    ///
    /// # Panics
    ///
    /// Panics if any weight is negative or the sum is not positive. Callers must
    /// only construct this from valid degree weights (`1/k`).
    pub(crate) fn new(mut weights: Vec<f64>) -> Self {
        assert!(
            !weights.iter().any(|&p| p < 0.0),
            "negative probability encountered"
        );
        let summed = weights.iter().sum::<f64>();
        assert!(summed > 0.0, "probabilities don't sum to a positive value");
        let count = weights.len();
        // Degree table size is small (≤ max fragment count); f64 scaling is normative.
        #[allow(
            clippy::cast_precision_loss,
            reason = "alias weights use f64 reciprocals as specified by URKit/ur-rs"
        )]
        let count_f = count as f64;
        for w in &mut weights {
            *w *= count_f / summed;
        }
        // Ordered partition: indices count-1 down to 0, small then large.
        let (mut small, mut large): (Vec<usize>, Vec<usize>) = (1..=count)
            .map(|j| count - j)
            .partition(|&j| weights.get(j).is_some_and(|&w| w < 1.0));

        let mut probs: Vec<f64> = alloc::vec![0.0; count];
        let mut aliases: Vec<u32> = alloc::vec![0; count];

        while !small.is_empty() && !large.is_empty() {
            let Some(a) = small.pop() else { break };
            let Some(g) = large.pop() else { break };
            reduce_alias_step(
                a,
                g,
                &mut weights,
                &mut probs,
                &mut aliases,
                &mut small,
                &mut large,
            );
        }

        while let Some(g) = large.pop() {
            if let Some(prob) = probs.get_mut(g) {
                *prob = 1.0;
            }
        }

        while let Some(a) = small.pop() {
            if let Some(prob) = probs.get_mut(a) {
                *prob = 1.0;
            }
        }

        Self { aliases, probs }
    }

    pub(crate) fn next(&self, xoshiro: &mut Xoshiro256) -> u32 {
        let r1 = xoshiro.next_double();
        let r2 = xoshiro.next_double();
        let n = self.probs.len();
        // Float scaling is part of the UR degree-selection definition.
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_precision_loss,
            clippy::cast_sign_loss,
            reason = "IEEE-754 unit-interval sampling matches ur-rs/URKit bit-for-bit"
        )]
        let i = {
            let n_f = n as f64;
            let scaled = n_f * r1;
            let raw = scaled as usize;
            raw.min(n.saturating_sub(1))
        };
        let Some(&prob) = self.probs.get(i) else {
            return 0;
        };
        if r2 < prob {
            u32::try_from(i).unwrap_or(0)
        } else {
            self.aliases.get(i).copied().unwrap_or(0)
        }
    }
}

fn reduce_alias_step(
    a: usize,
    g: usize,
    weights: &mut [f64],
    probs: &mut [f64],
    aliases: &mut [u32],
    small: &mut Vec<usize>,
    large: &mut Vec<usize>,
) {
    let Some(weight_a) = weights.get(a).copied() else {
        return;
    };
    if let Some(prob) = probs.get_mut(a) {
        *prob = weight_a;
    }
    if let Ok(alias) = u32::try_from(g) {
        if let Some(slot) = aliases.get_mut(a) {
            *slot = alias;
        }
    }
    let Some(weight_g) = weights.get_mut(g) else {
        return;
    };
    *weight_g += weight_a - 1.0;
    if *weight_g < 1.0 {
        small.push(g);
    } else {
        large.push(g);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::Xoshiro256;

    #[test]
    fn test_sampler() {
        let weights = vec![1.0, 2.0, 4.0, 8.0];
        let mut xoshiro = Xoshiro256::from("Wolf");
        let sampler = Weighted::new(weights);
        let expected = [
            3, 3, 3, 3, 3, 3, 3, 0, 2, 3, 3, 3, 3, 1, 2, 2, 1, 3, 3, 2, 3, 3, 1, 1, 2, 1, 1, 3, 1,
            3,
        ];
        for e in expected {
            assert_eq!(sampler.next(&mut xoshiro), e, "sampler next");
        }
    }
}
