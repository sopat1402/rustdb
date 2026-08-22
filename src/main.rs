//Code by Sohum Pathak
//sohum.pathak@protonmail.com

use std::fs::File;

use rustdb::db_errors::DbError;
use rustdb::index::Index;
use rustdb::page::DatabaseFile;
use rustdb::slotted_page::RECORD_SIZE;

fn make_record(id: u16, text: &[u8]) -> ([u8; RECORD_SIZE], usize) {
    let mut record = [0u8; RECORD_SIZE];

    record[0..2].copy_from_slice(&id.to_le_bytes());

    let end = 2 + text.len();
    record[2..end].copy_from_slice(text);

    (record, end)
}

fn open_database() -> Result<DatabaseFile, DbError> {
    let file = File::options()
        .read(true).write(true).create(true)
        .open("database.db")
        .map_err(|_| DbError::FileError)?;

    let page_metadata = File::options()
        .read(true).write(true).create(true)
        .open("pages.page")
        .map_err(|_| DbError::FileError)?;

    let btree = File::options()
        .read(true).write(true).create(true)
        .open("btree.tree")
        .map_err(|_| DbError::FileError)?;

    let size = file.metadata().map_err(|_| DbError::FileError)?.len();

    Ok(DatabaseFile {
        file,
        page_metadata,
        btree,
        size,
    })
}
fn main() {
    let _ = std::fs::remove_file("database.db");
    let _ = std::fs::remove_file("pages.page");
    let _ = std::fs::remove_file("btree.tree");
    let db = match open_database() {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Failed to open database files: {:?}", e);
            return;
        }
    };
    let mut index = match Index::new(db) {
        Ok(idx) => idx,
        Err(e) => {
            eprintln!("Failed to create index: {:?}", e);
            return;
        }
    };

}
