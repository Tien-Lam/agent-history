use rusqlite::Connection;
use tempfile::TempDir;

use super::connection::migrations;
use super::*;

mod connection;
mod filters;
mod notes;
mod refs;
mod stars;
mod tags;

fn open_fresh() -> (TempDir, Connection) {
    let tmp = TempDir::new().unwrap();
    let conn = open(&tmp.path().join("metadata.db")).unwrap();
    (tmp, conn)
}
