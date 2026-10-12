# inMemory-VectorDB

An in-memory vector database backed by an HNSW index. The server exposes a small TCP API and writes every mutation to an append-only JSON log so the index can be rebuilt after a restart.

## Requirements

- Rust 1.88 or newer
- Node.js 20 or newer for the TypeScript client

## Start the server

From the repository root:

```sh
cargo run -- server config.json
```

Running the binary without arguments starts the server with `config.json`. If that file does not exist, the binary writes the default configuration before starting.

The checked-in configuration listens on `127.0.0.1:7000` and stores data in `data/`. Change `bind_address` or `data_dir` when running the server in a different environment.

## Command-line client

The binary includes a basic client for smoke tests and scripts:

```sh
cargo run -- client 127.0.0.1:7000 insert 1,0 hello
cargo run -- client 127.0.0.1:7000 search 1,0 5
cargo run -- client 127.0.0.1:7000 stats
cargo run -- client 127.0.0.1:7000 remove 0
```

The insert command treats its optional payload as UTF-8. The client libraries accept arbitrary bytes.

## TCP protocol

The protocol is newline-delimited JSON. A client may keep one connection open and send multiple requests. Every request receives exactly one response line.

Requests:

```json
{"type":"insert","vector":[0.1,0.2,0.3],"data":[104,105]}
{"type":"search","vector":[0.1,0.2,0.3],"k":10}
{"type":"remove","id":0}
{"type":"stats"}
```

Successful responses have `ok: true`. Insert responses include `id`, search responses include `results`, and stats responses include `count` and optional `dim`. Failed requests have `ok: false` and an `error` string.

Search results contain the numeric ID, original payload bytes, and cosine `similarity`.

## Client libraries

### Rust

The async Rust client is in [client-lib/rust](client-lib/rust). Add it to an application with a local path dependency:

```toml
[dependencies]
inmemory-vectordb-client = { path = "../inMemory-VectorDB/client-lib/rust" }
tokio = { version = "1.53", features = ["macros", "net", "rt-multi-thread"] }
```

Example:

```rust
use inmemory_vectordb_client::Client;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
	let mut db = Client::connect("127.0.0.1:7000").await?;
	let id = db.insert(&[1.0, 0.0], b"hello").await?;
	let hits = db.search(&[1.0, 0.0], 5).await?;
	println!("inserted {id}, found {} result(s)", hits.len());
	Ok(())
}
```

Build it independently with `cargo check --manifest-path client-lib/rust/Cargo.toml`.

### TypeScript

The Bun-based TypeScript client is in [client-lib/typescript](client-lib/typescript). Install and build it with:

```sh
cd client-lib/typescript
bun install
bun run build
```

Example:

```ts
import { Client } from "@inmemory-vectordb/client";

const db = await Client.connect("127.0.0.1:7000");
const id = await db.insert([1, 0], Buffer.from("hello"));
const hits = await db.search([1, 0], 5);
console.log(id, hits);
db.close();
```

## Persistence model

Each successful insert and remove is flushed and synced to `data/operations.log`. On startup, the server creates a new HNSW index and replays the log in order. IDs are the append order of inserts, so the configured HNSW seed should remain unchanged while replaying an existing log.

The log is the source of truth, but it is not a compacted snapshot. For a long-running database, add log rotation or snapshotting before the log becomes expensive to replay. The current TCP server also has no authentication or TLS; keep it on a trusted network or put it behind an authenticated transport.

## Development

Run the server tests and format check from the repository root:

```sh
cargo fmt --all -- --check
cargo test
```

The Rust client has its own manifest and is intentionally not part of the server package. Run its checks separately with:

```sh
cargo fmt --manifest-path client-lib/rust/Cargo.toml -- --check
cargo check --manifest-path client-lib/rust/Cargo.toml
```