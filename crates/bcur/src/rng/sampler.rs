//! Walker's alias method sampler (ordered partition matching ur-rs).

use alloc::vec::Vec;

use super::Xoshiro256;

/// Weighted discrete sampler over `0..weights.len()`.
#[derive(Debug)]
pub(crate) struct Weighted {
    aliases: Vec<u32>,
    probs: Vec<f64>,
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]
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
        for w in &mut weights {
            *w *= count as f64 / summed;
        }
        // Ordered partition: indices count-1 down to 0, small then large.
        let (mut s, mut l): (Vec<usize>, Vec<usize>) = (1..=count)
            .map(|j| count - j)
            .partition(|&j| weights[j] < 1.0);

        let mut probs: Vec<f64> = alloc::vec![0.0; count];
        let mut aliases: Vec<u32> = alloc::vec![0; count];

        while !s.is_empty() && !l.is_empty() {
            let a = s.remove(s.len() - 1);
            let g = l.remove(l.len() - 1);
            probs[a] = weights[a];
            aliases[a] = g as u32;
            weights[g] += weights[a] - 1.0;
            if weights[g] < 1.0 {
                s.push(g);
            } else {
                l.push(g);
            }
        }

        while !l.is_empty() {
            let g = l.remove(l.len() - 1);
            probs[g] = 1.0;
        }

        while !s.is_empty() {
            let a = s.remove(s.len() - 1);
            probs[a] = 1.0;
        }

        Self { aliases, probs }
    }

    pub(crate) fn next(&self, xoshiro: &mut Xoshiro256) -> u32 {
        let r1 = xoshiro.next_double();
        let r2 = xoshiro.next_double();
        let n = self.probs.len();
        let i = (n as f64 * r1) as usize;
        if r2 < self.probs[i] {
            i as u32
        } else {
            self.aliases[i]
        }
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
            assert_eq!(sampler.next(&mut xoshiro), e);
        }
    }
}
