//Code by Sohum Pathak
//sohum.pathak@protonmail.com

mod crc32;
mod index;
mod db_errors;
mod b_plus_tree;
mod page;
mod lru_cache;
mod slotted_page;
mod buffer_pool;

use std::fs::File;

use db_errors::DbError;
use index::Index;
use page::DatabaseFile;
use slotted_page::RECORD_SIZE;

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
    println!("=== PERSISTENT INDEXING TEST ===");
    let _ = std::fs::remove_file("database.db");
    let _ = std::fs::remove_file("pages.page");
    let _ = std::fs::remove_file("btree.tree");

    let db = match open_database() {
        Ok(db) => db,
        Err(e) => {
            eprintln!("failed to open database files: {:?}", e);
            return;
        }
    };

    let mut index = match Index::new(db) {
        Ok(idx) => idx,
        Err(e) => {
            eprintln!("failed to create index: {:?}", e);
            return;
        }
    };

    let (record1, size1) = make_record(1, b"hello");

    match index.write_record(&record1, size1) {
        Ok(_) => println!("write record 1: hello"),
        Err(e) => {
            eprintln!("failed to write record 1: {:?}", e);
            return;
        }
    }
    let (record2, size2) = make_record(2, b"world");

    match index.write_record(&record2, size2) {
        Ok(_) => println!("write record 2: world"),
        Err(e) => {
            eprintln!("failed to write record 2: {:?}", e);
            return;
        }
    }
    let (record3, size3) = make_record(3, b"database");

    match index.write_record(&record3, size3) {
        Ok(_) => println!("write record 3: database"),
        Err(e) => {
            eprintln!("failed to write record 3: {:?}", e);
            return;
        }
    }
    let mut read_buffer = [0u8; RECORD_SIZE];
    match index.get_record(1, &mut read_buffer) {
        Ok(size) => {
            assert_eq!(&read_buffer[2..size], b"hello");
            println!("get record 1: hello");
        }

        Err(e) => {
            eprintln!("failed to get record 1: {:?}", e);
            return;
        }
    }
    read_buffer = [0u8; RECORD_SIZE];

    match index.get_record(2, &mut read_buffer) {
        Ok(size) => {
            assert_eq!(&read_buffer[2..size], b"world");
            println!("get record 2: world");
        }

        Err(e) => {
            eprintln!("failed to get record 2: {:?}", e);
            return;
        }
    }
    read_buffer = [0u8; RECORD_SIZE];

    match index.get_record(3, &mut read_buffer) {
        Ok(size) => {
            assert_eq!(&read_buffer[2..size], b"database");
            println!("get record 3: database");
        }

        Err(e) => {
            eprintln!("failed to get record 3: {:?}", e);
            return;
        }
    }
    let (updated, updated_size) = make_record(2, b"updated");

    match index.update_record(2, &updated, updated_size) {
        Ok(_) => println!("update record 2: updated"),
        Err(e) => {
            eprintln!("failed to update record 2: {:?}", e);
            return;
        }
    }
    read_buffer = [0u8; RECORD_SIZE];

    match index.get_record(2, &mut read_buffer) {
        Ok(size) => {
            assert_eq!(&read_buffer[2..size], b"updated");
            println!("get updated record 2: updated");
        }

        Err(e) => {
            eprintln!("failed to read updated record 2: {:?}", e);
            return;
        }
    }
    match index.delete_record(2) {
        Ok(()) => println!("delete record 2: OK"),
        Err(e) => {
            eprintln!("failed to delete record 2: {:?}", e);
            return;
        }
    }
    read_buffer = [0u8; RECORD_SIZE];

    match index.get_record(2, &mut read_buffer) {
        Ok(_) => {
            eprintln!("ERROR: deleted record 2 was found");
            return;
        }

        Err(DbError::RecordAbsent) => {
            println!("get deleted record 2: correctly returned RecordAbsent");
        }

        Err(e) => {
            eprintln!("wrong error for deleted record: {:?}", e);
            return;
        }
    }
    let (record4, size4) = make_record(4, b"new record");

    match index.write_record(&record4, size4) {
        Ok(_) => println!("write record 4 after deletion: OK"),
        Err(e) => {
            eprintln!("failed to write record 4: {:?}", e);
            return;
        }
    }
    read_buffer = [0u8; RECORD_SIZE];

    match index.get_record(4, &mut read_buffer) {
        Ok(size) => {
            assert_eq!(&read_buffer[2..size], b"new record");
            println!("get record 4: new record");
        }

        Err(e) => {
            eprintln!("failed to get record 4: {:?}", e);
            return;
        }
    }
    match index.pool.evict_all() {
        Ok(()) => println!("flushed buffer pool"),
        Err(e) => {
            eprintln!("failed to flush buffer pool: {:?}", e);
            return;
        }
    }
    match index.tree.serialise(&mut index.pool.db_file) {
        Ok(()) => println!("serialised B+ tree"),
        Err(e) => {
            eprintln!("failed to serialise B+ tree: {:?}", e);
            return;
        }
    }
    drop(index);
    println!("dropped entire index");
    let db = match open_database() {
        Ok(db) => db,
        Err(e) => {
            eprintln!("failed to reopen database files: {:?}", e);
            return;
        }
    };
    let mut index = match Index::new(db) {
        Ok(idx) => idx,
        Err(e) => {
            eprintln!("failed to create index: {:?}", e);
            return;
        }
    };
    read_buffer = [0u8; RECORD_SIZE];

    match index.get_record(1, &mut read_buffer) {
        Ok(size) => {
            assert_eq!(&read_buffer[2..size], b"hello");

            println!("record 1 survived restart: hello");
        }

        Err(e) => {
            eprintln!("record 1 was lost after restart: {:?}", e);
            return;
        }
    }
    read_buffer = [0u8; RECORD_SIZE];

    match index.get_record(2, &mut read_buffer) {
        Ok(_) => {
            eprintln!("ERROR: deleted record 2 survived restart");
            return;
        }

        Err(DbError::RecordAbsent) => {
            println!("deleted record 2 remained deleted after restart");
        }

        Err(e) => {
            eprintln!("wrong error for deleted record after restart: {:?}", e);
            return;
        }
    }
    read_buffer = [0u8; RECORD_SIZE];

    match index.get_record(3, &mut read_buffer) {
        Ok(size) => {
            assert_eq!(&read_buffer[2..size], b"database");

            println!("record 3 survived restart: database");
        }

        Err(e) => {
            eprintln!("record 3 was lost after restart: {:?}", e);
            return;
        }
    }
    read_buffer = [0u8; RECORD_SIZE];

    match index.get_record(4, &mut read_buffer) {
        Ok(size) => {
            assert_eq!(&read_buffer[2..size], b"new record");

            println!("record 4 survived restart: new record");
        }

        Err(e) => {
            eprintln!("record 4 was lost after restart: {:?}", e);
            return;
        }
    }
    let (record5, size5) = make_record(5, b"after restart");

    match index.write_record(&record5, size5) {
        Ok(_) => println!("write record 5 after restart: OK"),
        Err(e) => {
            eprintln!("failed to write record 5 after restart: {:?}", e);
            return;
        }
    }


    read_buffer = [0u8; RECORD_SIZE];

    match index.get_record(5, &mut read_buffer) {
        Ok(size) => {
            assert_eq!(&read_buffer[2..size], b"after restart");

            println!("record 5 written successfully after restart");
        }

        Err(e) => {
            eprintln!("failed to get record 5 after restart: {:?}", e);
            return;
        }
    }


    println!();
    println!("=== PERSISTENT INDEXING TEST PASSED ===");
}
