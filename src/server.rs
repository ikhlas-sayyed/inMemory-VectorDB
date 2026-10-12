use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

use crate::database::{Database, DatabaseError};

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    Insert { vector: Vec<f32>, data: Vec<u8> },
    Search { vector: Vec<f32>, k: usize },
    Remove { id: usize },
    Stats,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Response {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub results: Option<Vec<crate::engine::SearchResult>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dim: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Response {
    fn success() -> Self {
        Self {
            ok: true,
            id: None,
            results: None,
            count: None,
            dim: None,
            error: None,
        }
    }

    fn error(error: impl Into<String>) -> Self {
        Self {
            ok: false,
            id: None,
            results: None,
            count: None,
            dim: None,
            error: Some(error.into()),
        }
    }
}

pub async fn run(database: Arc<Mutex<Database>>, bind_address: &str) -> std::io::Result<()> {
    let listener = TcpListener::bind(bind_address).await?;
    println!("inMemory-VectorDB listening on {bind_address}");
    loop {
        let (stream, peer) = listener.accept().await?;
        let database = Arc::clone(&database);
        tokio::spawn(async move {
            if let Err(error) = handle_connection(stream, database).await {
                eprintln!("connection {peer} failed: {error}");
            }
        });
    }
}

async fn handle_connection(
    stream: TcpStream,
    database: Arc<Mutex<Database>>,
) -> std::io::Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    while let Some(line) = lines.next_line().await? {
        let response = match serde_json::from_str::<Request>(&line) {
            Ok(request) => execute(request, &database),
            Err(error) => Response::error(format!("invalid request: {error}")),
        };
        let encoded = serde_json::to_vec(&response)
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        writer.write_all(&encoded).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;
    }
    Ok(())
}

fn execute(request: Request, database: &Arc<Mutex<Database>>) -> Response {
    let mut database = match database.lock() {
        Ok(database) => database,
        Err(_) => return Response::error("database lock poisoned"),
    };

    let result = match request {
        Request::Insert { vector, data } => database.insert(vector, data).map(|id| {
            let mut response = Response::success();
            response.id = Some(id);
            response
        }),
        Request::Search { vector, k } => database.search(&vector, k).map(|results| {
            let mut response = Response::success();
            response.results = Some(results);
            response
        }),
        Request::Remove { id } => database.remove(id).map(|_| Response::success()),
        Request::Stats => {
            let mut response = Response::success();
            response.count = Some(database.len());
            response.dim = database.dim();
            Ok(response)
        }
    };

    result.unwrap_or_else(|error: DatabaseError| Response::error(error.to_string()))
}

pub struct Client {
    reader: BufReader<tokio::net::tcp::OwnedReadHalf>,
    writer: tokio::net::tcp::OwnedWriteHalf,
}

impl Client {
    pub async fn connect(address: &str) -> std::io::Result<Self> {
        let (reader, writer) = TcpStream::connect(address).await?.into_split();
        Ok(Self {
            reader: BufReader::new(reader),
            writer,
        })
    }

    pub async fn request(&mut self, request: &Request) -> std::io::Result<Response> {
        let mut encoded = serde_json::to_vec(request)
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        encoded.push(b'\n');
        self.writer.write_all(&encoded).await?;
        self.writer.flush().await?;

        let mut line = String::new();
        self.reader.read_line(&mut line).await?;
        if line.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "server closed connection",
            ));
        }
        serde_json::from_str(&line).map_err(|error| std::io::Error::other(error.to_string()))
    }
}
