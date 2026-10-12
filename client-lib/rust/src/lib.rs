use std::io;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{
    TcpStream,
    tcp::{OwnedReadHalf, OwnedWriteHalf},
};

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    Insert { vector: Vec<f32>, data: Vec<u8> },
    Search { vector: Vec<f32>, k: usize },
    Remove { id: usize },
    Stats,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SearchResult {
    pub id: usize,
    pub data: Vec<u8>,
    pub similarity: f32,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Response {
    pub ok: bool,
    pub id: Option<usize>,
    pub results: Option<Vec<SearchResult>>,
    pub count: Option<usize>,
    pub dim: Option<usize>,
    pub error: Option<String>,
}

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Json(serde_json::Error),
    Server(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "I/O error: {error}"),
            Self::Json(error) => write!(formatter, "protocol error: {error}"),
            Self::Server(error) => write!(formatter, "server error: {error}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

pub struct Client {
    reader: BufReader<OwnedReadHalf>,
    writer: OwnedWriteHalf,
}

impl Client {
    pub async fn connect(address: &str) -> Result<Self, Error> {
        let (reader, writer) = TcpStream::connect(address).await?.into_split();
        Ok(Self {
            reader: BufReader::new(reader),
            writer,
        })
    }

    pub async fn request(&mut self, request: &Request) -> Result<Response, Error> {
        let mut message = serde_json::to_vec(request)?;
        message.push(b'\n');
        self.writer.write_all(&message).await?;
        self.writer.flush().await?;

        let mut line = String::new();
        if self.reader.read_line(&mut line).await? == 0 {
            return Err(Error::Io(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "server closed the connection",
            )));
        }
        let response: Response = serde_json::from_str(&line)?;
        if response.ok {
            Ok(response)
        } else {
            Err(Error::Server(
                response
                    .error
                    .unwrap_or_else(|| "unknown server error".to_owned()),
            ))
        }
    }

    pub async fn insert(&mut self, vector: &[f32], data: &[u8]) -> Result<usize, Error> {
        let response = self
            .request(&Request::Insert {
                vector: vector.to_vec(),
                data: data.to_vec(),
            })
            .await?;
        response
            .id
            .ok_or_else(|| Error::Server("insert response did not include an id".to_owned()))
    }

    pub async fn search(&mut self, vector: &[f32], k: usize) -> Result<Vec<SearchResult>, Error> {
        let response = self
            .request(&Request::Search {
                vector: vector.to_vec(),
                k,
            })
            .await?;
        Ok(response.results.unwrap_or_default())
    }

    pub async fn remove(&mut self, id: usize) -> Result<(), Error> {
        self.request(&Request::Remove { id }).await.map(|_| ())
    }

    pub async fn stats(&mut self) -> Result<(usize, Option<usize>), Error> {
        let response = self.request(&Request::Stats).await?;
        Ok((response.count.unwrap_or(0), response.dim))
    }
}
