mod connection;
mod queries;
mod types;

pub use connection::{get_db_connection, init_db};
pub use queries::*;
pub use types::{ClipboardEntry, SearchParams};

use std::time::{SystemTime, UNIX_EPOCH};

#[must_use]
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should go forward - problem with system clock")
        .as_secs()
}
