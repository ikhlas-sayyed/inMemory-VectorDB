mod config;
mod database;
mod engine;
mod server;

use std::error::Error;
use std::sync::{Arc, Mutex};

use config::DatabaseConfig;
use database::Database;
use server::{Client, Request};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = std::env::args().skip(1);
    match arguments.next().as_deref() {
        Some("client") => run_client(arguments.collect()).await?,
        Some("server") | None => run_server(arguments.next().as_deref()).await?,
        Some(command) => return Err(format!("unknown command: {command}").into()),
    }
    Ok(())
}

async fn run_server(config_path: Option<&str>) -> Result<(), Box<dyn Error>> {
    let config_path = config_path.unwrap_or("config.json");
    let config = load_config(config_path)?;
    let database = Arc::new(Mutex::new(Database::open(&config)?));
    server::run(database, &config.bind_address).await?;
    Ok(())
}

async fn run_client(arguments: Vec<String>) -> Result<(), Box<dyn Error>> {
    let address = arguments.first().ok_or("client address is required")?;
    let command = arguments.get(1).ok_or("client command is required")?;
    let mut client = Client::connect(address).await?;
    let request = match command.as_str() {
        "insert" => Request::Insert {
            vector: parse_vector(arguments.get(2).ok_or("insert vector is required")?)?,
            data: arguments
                .get(3)
                .map_or_else(Vec::new, |data| data.as_bytes().to_vec()),
        },
        "search" => Request::Search {
            vector: parse_vector(arguments.get(2).ok_or("search vector is required")?)?,
            k: match arguments.get(3) {
                Some(value) => value.parse().map_err(|_| "invalid k")?,
                None => 10,
            },
        },
        "remove" => Request::Remove {
            id: arguments.get(2).ok_or("remove id is required")?.parse()?,
        },
        "stats" => Request::Stats,
        _ => return Err(format!("unknown client command: {command}").into()),
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&client.request(&request).await?)?
    );
    Ok(())
}

fn parse_vector(value: &str) -> Result<Vec<f32>, Box<dyn Error>> {
    value.split(',').map(|item| Ok(item.parse()?)).collect()
}

fn load_config(path: &str) -> Result<DatabaseConfig, Box<dyn Error>> {
    match DatabaseConfig::load(path) {
        Ok(config) => Ok(config),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            DatabaseConfig::write_default(path)?;
            Ok(DatabaseConfig::default())
        }
        Err(error) => Err(error.into()),
    }
}
