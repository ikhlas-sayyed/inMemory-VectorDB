use super::error::{HnswError, Result};

#[derive(Clone, Debug)]
pub struct Config {
    /// Neighbors kept per node on layers above 0.
    pub m: usize,
    /// Neighbors kept per node on layer 0 (usually 2 * m).
    pub m_max0: usize,
    /// Candidate list size while building.
    pub ef_construction: usize,
    /// Fixed RNG seed for reproducible graphs.
    pub seed: Option<u64>,
}

impl Default for Config {
    fn default() -> Self {
        Self::new(16, 200)
    }
}

impl Config {
    pub fn new(m: usize, ef_construction: usize) -> Self {
        Self {
            m,
            m_max0: m.saturating_mul(2),
            ef_construction,
            seed: None,
        }
    }

    pub fn with_seed(mut self, seed: u64) -> Self {
        self.seed = Some(seed);
        self
    }

    pub fn with_m_max0(mut self, m_max0: usize) -> Self {
        self.m_max0 = m_max0;
        self
    }

    pub(crate) fn validate(&self) -> Result<()> {
        if self.m < 2 {
            return Err(HnswError::InvalidConfig("m must be >= 2"));
        }
        if self.m_max0 < self.m {
            return Err(HnswError::InvalidConfig("m_max0 must be >= m"));
        }
        if self.ef_construction < self.m {
            return Err(HnswError::InvalidConfig("ef_construction must be >= m"));
        }
        Ok(())
    }

    /// Level multiplier 1 / ln(m), from the HNSW paper.
    pub(crate) fn level_mult(&self) -> f64 {
        1.0 / (self.m as f64).ln()
    }

    pub(crate) fn max_neighbors(&self, layer: usize) -> usize {
        if layer == 0 { self.m_max0 } else { self.m }
    }
}
