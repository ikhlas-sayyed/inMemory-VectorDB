use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::engine::Config as EngineConfig;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DatabaseConfig {
    pub bind_address: String,
    pub data_dir: PathBuf,
    pub m: usize,
    pub m_max0: usize,
    pub ef_construction: usize,
    pub ef_search: usize,
    pub seed: Option<u64>,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            bind_address: "127.0.0.1:7000".to_owned(),
            data_dir: PathBuf::from("data"),
            m: 16,
            m_max0: 32,
            ef_construction: 200,
            ef_search: 100,
            seed: Some(42),
        }
    }
}

impl DatabaseConfig {
    pub fn engine_config(&self) -> EngineConfig {
        let mut config = EngineConfig::new(self.m, self.ef_construction).with_m_max0(self.m_max0);
        if let Some(seed) = self.seed {
            config = config.with_seed(seed);
        }
        config
    }

    pub fn load(path: impl AsRef<Path>) -> io::Result<Self> {
        let contents = fs::read_to_string(path)?;
        serde_json::from_str(&contents)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }

    pub fn write_default(path: impl AsRef<Path>) -> io::Result<()> {
        let contents = serde_json::to_string_pretty(&Self::default())
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        fs::write(path, format!("{contents}\n"))
    }
}
