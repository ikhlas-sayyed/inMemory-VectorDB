use std::fs::{File, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};

use serde::{Deserialize, Serialize};

use crate::config::DatabaseConfig;
use crate::engine::{Hnsw, HnswError, SearchResult};

#[derive(Debug)]
pub enum DatabaseError {
    Io(io::Error),
    Json(serde_json::Error),
    Engine(HnswError),
}

impl std::fmt::Display for DatabaseError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "I/O error: {error}"),
            Self::Json(error) => write!(formatter, "log format error: {error}"),
            Self::Engine(error) => write!(formatter, "engine error: {error}"),
        }
    }
}

impl std::error::Error for DatabaseError {}

impl From<io::Error> for DatabaseError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for DatabaseError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl From<HnswError> for DatabaseError {
    fn from(error: HnswError) -> Self {
        Self::Engine(error)
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Operation {
    Insert { vector: Vec<f32>, data: Vec<u8> },
    Remove { id: usize },
}

pub struct Database {
    index: Hnsw,
    log: File,
    ef_search: usize,
}

impl Database {
    pub fn open(config: &DatabaseConfig) -> Result<Self, DatabaseError> {
        std::fs::create_dir_all(&config.data_dir)?;
        let log_path = config.data_dir.join("operations.log");
        let replay_log = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&log_path)?;
        let mut index = Hnsw::new(config.engine_config())?;

        for (line_number, line) in BufReader::new(replay_log).lines().enumerate() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let operation: Operation = serde_json::from_str(&line).map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("invalid operation at log line {}: {error}", line_number + 1),
                )
            })?;
            apply_operation(&mut index, operation)?;
        }

        let log = OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path)?;
        Ok(Self {
            index,
            log,
            ef_search: config.ef_search,
        })
    }

    pub fn insert(&mut self, vector: Vec<f32>, data: Vec<u8>) -> Result<usize, DatabaseError> {
        let id = self.index.insert(vector.clone(), data.clone())?;
        self.append(&Operation::Insert { vector, data })?;
        Ok(id)
    }

    pub fn search(&self, vector: &[f32], k: usize) -> Result<Vec<SearchResult>, DatabaseError> {
        Ok(self.index.search(vector, k, self.ef_search)?)
    }

    pub fn remove(&mut self, id: usize) -> Result<(), DatabaseError> {
        self.index.remove(id)?;
        self.append(&Operation::Remove { id })
    }

    pub fn len(&self) -> usize {
        self.index.len()
    }

    pub fn dim(&self) -> Option<usize> {
        self.index.dim()
    }

    fn append(&mut self, operation: &Operation) -> Result<(), DatabaseError> {
        serde_json::to_writer(&mut self.log, operation)?;
        self.log.write_all(b"\n")?;
        self.log.flush()?;
        self.log.sync_data()?;
        Ok(())
    }
}

fn apply_operation(index: &mut Hnsw, operation: Operation) -> Result<(), DatabaseError> {
    match operation {
        Operation::Insert { vector, data } => {
            index.insert(vector, data)?;
        }
        Operation::Remove { id } => index.remove(id)?,
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_config() -> DatabaseConfig {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        DatabaseConfig {
            data_dir: std::env::temp_dir().join(format!("inmemory-vector-db-{suffix}")),
            ..DatabaseConfig::default()
        }
    }

    #[test]
    fn reopens_from_operation_log() {
        let config = test_config();
        {
            let mut database = Database::open(&config).unwrap();
            assert_eq!(database.insert(vec![1.0, 0.0], vec![7, 8]).unwrap(), 0);
            assert_eq!(database.insert(vec![0.0, 1.0], vec![9]).unwrap(), 1);
        }
        let database = Database::open(&config).unwrap();
        let results = database.search(&[1.0, 0.0], 1).unwrap();
        assert_eq!(results[0].id, 0);
        assert_eq!(results[0].data, vec![7, 8]);
        assert_eq!(database.len(), 2);
        std::fs::remove_dir_all(config.data_dir).unwrap();
    }
}
