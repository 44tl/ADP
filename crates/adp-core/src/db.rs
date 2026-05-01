//! Database connection and migration management.
//!
//! Uses `refinery` with `rusqlite` for schema migrations, then hands off
//! to `libsql` for async runtime access. Both operate on the same SQLite
//! file, so this is safe as long as migrations run before runtime queries.

use crate::error::{AdpError, Result};
use std::path::Path;
use tracing::{info, instrument};

/// Embedded migrations from `adp-core/migrations/`.
mod embedded {
    use refinery::embed_migrations;
    embed_migrations!("migrations");
}

/// Open (or create) a local libSQL database, run pending migrations, and
/// return a [`libsql::Connection`].
///
/// # Example
///
/// ```no_run
/// use adp_core::db::open_database;
///
/// # async fn example() -> adp_core::Result<()> {
/// let conn = open_database("./adp.db").await?;
/// # Ok(())
/// # }
/// ```
#[instrument]
pub async fn open_database<P: AsRef<Path>>(path: P) -> Result<libsql::Connection> {
    let path = path.as_ref();
    info!(db_path = %path.display(), "opening database");

    // Run migrations via rusqlite (synchronous, one-time at startup).
    run_migrations(path)?;

    // Open libsql connection for async runtime.
    let db = libsql::Builder::new_local(path)
        .build()
        .await
        .map_err(|e| AdpError::StoreError(format!("libsql build failed: {e}")))?;

    let conn = db
        .connect()
        .map_err(|e| AdpError::StoreError(format!("libsql connect failed: {e}")))?;

    info!(db_path = %path.display(), "database ready");
    Ok(conn)
}

fn run_migrations(path: &Path) -> Result<()> {
    let mut conn = rusqlite::Connection::open(path)
        .map_err(|e| AdpError::StoreError(format!("rusqlite open failed: {e}")))?;

    embedded::migrations::runner()
        .run(&mut conn)
        .map_err(|e| AdpError::StoreError(format!("migration failed: {e}")))?;

    info!("migrations applied successfully");
    Ok(())
}
