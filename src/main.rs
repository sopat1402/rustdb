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

fn main() {
    println!("=== INDEXING TEST ===");

    // ------------------------------------------------------------
    // 1. Start with a fresh database
    // ------------------------------------------------------------

    let _ = std::fs::remove_file("database.db");

    let file = match File::options()
        .read(true)
        .write(true)
        .create(true)
        .open("database.db")
    {
        Ok(f) => f,
        Err(e) => {
            eprintln!("failed to create database: {e}");
            return;
        }
    };

    let db = DatabaseFile {
        file,
        size: 0,
    };

    let mut index = Index::new(db);

    println!("created index");


    // ------------------------------------------------------------
    // 2. Write several records
    // ------------------------------------------------------------

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


    // ------------------------------------------------------------
    // 3. Read records through the index
    // ------------------------------------------------------------

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


    // ------------------------------------------------------------
    // 4. Update record 2
    // ------------------------------------------------------------

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


    // ------------------------------------------------------------
    // 5. Delete record 2
    // ------------------------------------------------------------

    match index.delete_record(2) {
        Ok(()) => println!("delete record 2: OK"),
        Err(e) => {
            eprintln!("failed to delete record 2: {:?}", e);
            return;
        }
    }


    // ------------------------------------------------------------
    // 6. Verify deleted record is absent
    // ------------------------------------------------------------

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


    // ------------------------------------------------------------
    // 7. Verify remaining records still exist
    // ------------------------------------------------------------

    read_buffer = [0u8; RECORD_SIZE];

    match index.get_record(1, &mut read_buffer) {
        Ok(size) => {
            assert_eq!(&read_buffer[2..size], b"hello");
            println!("record 1 survived deletion of record 2");
        }

        Err(e) => {
            eprintln!("record 1 disappeared: {:?}", e);
            return;
        }
    }


    read_buffer = [0u8; RECORD_SIZE];

    match index.get_record(3, &mut read_buffer) {
        Ok(size) => {
            assert_eq!(&read_buffer[2..size], b"database");
            println!("record 3 survived deletion of record 2");
        }

        Err(e) => {
            eprintln!("record 3 disappeared: {:?}", e);
            return;
        }
    }


    // ------------------------------------------------------------
    // 8. Write after deletion
    // ------------------------------------------------------------

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


    println!();
    println!("=== INDEXING TEST PASSED ===");
}
