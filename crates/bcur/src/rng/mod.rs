//! Xoshiro256** seeding and Walker's alias sampler used by fountain codes.

mod sampler;
mod xoshiro;

pub(crate) use sampler::Weighted;
pub(crate) use xoshiro::Xoshiro256;

#[cfg(test)]
pub(crate) use xoshiro::test_utils;
