mod connection;
mod queries;
mod types;

pub use connection::{get_db_connection, init_db};
pub use queries::*;
pub use types::{ClipboardEntry, ContentType, SearchParams};

pub fn hash_bytes(data: &[u8]) -> String {
    blake3::hash(data).to_hex().to_string()
}
