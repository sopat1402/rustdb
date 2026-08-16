mod page;
mod slotted_page;
mod buffer_pool;

use std::fs::File;

use page::{DatabaseFile, PAGE_SIZE, PageHeader, PageType};
use slotted_page::{Page, RECORD_SIZE, RecordError};

fn main() {
    println!("=== PERSISTENCE TEST ===");

    // ------------------------------------------------------------
    // 1. Start with a completely fresh database
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

    let mut db = DatabaseFile {
        file,
        size: 0,
    };

    println!("Created database");


    // ------------------------------------------------------------
    // 2. Allocate page 0
    // ------------------------------------------------------------

    let page_id = match db.allocate_page() {
        Ok(id) => {
            println!("Allocated page {id}");
            id
        }

        Err(e) => {
            eprintln!("failed to allocate page: {e}");
            return;
        }
    };


    // ------------------------------------------------------------
    // 3. Create a Page in memory
    // ------------------------------------------------------------

    let header = PageHeader::new(page_id, PageType::Data);
    let buffer = [0u8; PAGE_SIZE];

    let mut page = Page::new(header, buffer);

    println!("Created page in memory");


    // ------------------------------------------------------------
    // 4. Write records into the page
    // ------------------------------------------------------------

    let mut record1 = [0u8; RECORD_SIZE];
    record1[0..2].copy_from_slice(&1u16.to_le_bytes());
    record1[2..7].copy_from_slice(b"hello");

    match page.write_record(1, &record1, 7) {
        Ok(size) => println!("write record 1: OK ({size} bytes)"),
        Err(e) => {
            eprintln!("write record 1 failed: {:?}", e);
            return;
        }
    }


    let mut record2 = [0u8; RECORD_SIZE];
    record2[0..2].copy_from_slice(&2u16.to_le_bytes());
    record2[2..7].copy_from_slice(b"world");

    match page.write_record(2, &record2, 7) {
        Ok(size) => println!("write record 2: OK ({size} bytes)"),
        Err(e) => {
            eprintln!("write record 2 failed: {:?}", e);
            return;
        }
    }


    // ------------------------------------------------------------
    // 5. Serialize the modified header into the page buffer
    // ------------------------------------------------------------

    page.header.serialise(&mut page.buffer);

    println!("Serialized page");


    // ------------------------------------------------------------
    // 6. Write the entire page to disk
    // ------------------------------------------------------------

    match db.write_page(page_id, &page.buffer) {
        Ok(()) => println!("write page 0 to disk: OK"),
        Err(e) => {
            eprintln!("write page failed: {e}");
            return;
        }
    }


    // ------------------------------------------------------------
    // 7. DESTROY THE IN-MEMORY PAGE
    //
    // This is important.
    //
    // Everything we're about to read must come from database.db.
    // ------------------------------------------------------------

    drop(page);

    println!("Dropped in-memory page");


    // ------------------------------------------------------------
    // 8. Load the page back from disk
    // ------------------------------------------------------------

    let mut page = match Page::load(page_id as u16, &db) {
        Ok(p) => {
            println!("Loaded page 0 from disk: OK");
            p
        }

        Err(e) => {
            eprintln!("failed to load page: {:?}", e);
            return;
        }
    };


    // ------------------------------------------------------------
    // 9. Verify record 1 survived persistence
    // ------------------------------------------------------------

    let mut read_buffer = [0u8; RECORD_SIZE];

    match page.read_record(1, &mut read_buffer) {
        Ok(size) => {
            println!(
                "read persisted record 1: OK ({size} bytes): {:?}",
                &read_buffer[2..size]
            );

            if &read_buffer[2..size] != b"hello" {
                eprintln!("ERROR: record 1 contents are wrong");
                return;
            }
        }

        Err(e) => {
            eprintln!("failed to read persisted record 1: {:?}", e);
            return;
        }
    }


    // ------------------------------------------------------------
    // 10. Verify record 2 survived persistence
    // ------------------------------------------------------------

    read_buffer = [0u8; RECORD_SIZE];

    match page.read_record(2, &mut read_buffer) {
        Ok(size) => {
            println!(
                "read persisted record 2: OK ({size} bytes): {:?}",
                &read_buffer[2..size]
            );

            if &read_buffer[2..size] != b"world" {
                eprintln!("ERROR: record 2 contents are wrong");
                return;
            }
        }

        Err(e) => {
            eprintln!("failed to read persisted record 2: {:?}", e);
            return;
        }
    }


    // ------------------------------------------------------------
    // 11. Modify the page
    // ------------------------------------------------------------

    let mut updated = [0u8; RECORD_SIZE];
    updated[0..2].copy_from_slice(&1u16.to_le_bytes());
    updated[2..9].copy_from_slice(b"updated");

    match page.update_record(1, &updated, 9) {
        Ok(size) => println!("update record 1: OK ({size} bytes)"),
        Err(e) => {
            eprintln!("update failed: {:?}", e);
            return;
        }
    }


    // ------------------------------------------------------------
    // 12. Delete record 2
    // ------------------------------------------------------------

    match page.delete_record(2) {
        Ok(()) => println!("delete record 2: OK"),
        Err(e) => {
            eprintln!("delete failed: {:?}", e);
            return;
        }
    }


    // ------------------------------------------------------------
    // 13. Persist modified page
    // ------------------------------------------------------------

    page.header.serialise(&mut page.buffer);

    match db.write_page(page_id, &page.buffer) {
        Ok(()) => println!("write modified page to disk: OK"),
        Err(e) => {
            eprintln!("failed to write modified page: {e}");
            return;
        }
    }


    // ------------------------------------------------------------
    // 14. Destroy it AGAIN
    // ------------------------------------------------------------

    drop(page);

    println!("Dropped modified in-memory page");


    // ------------------------------------------------------------
    // 15. Load it AGAIN
    // ------------------------------------------------------------

    let page = match Page::load(page_id as u16, &db) {
        Ok(p) => {
            println!("Reloaded modified page from disk: OK");
            p
        }

        Err(e) => {
            eprintln!("failed to reload page: {:?}", e);
            return;
        }
    };


    // ------------------------------------------------------------
    // 16. Verify update survived
    // ------------------------------------------------------------

    let mut read_buffer = [0u8; RECORD_SIZE];

    match page.read_record(1, &mut read_buffer) {
        Ok(size) => {
            println!(
                "read updated persisted record 1: OK ({size} bytes): {:?}",
                &read_buffer[2..size]
            );

            if &read_buffer[2..size] != b"updated" {
                eprintln!("ERROR: updated record is wrong");
                return;
            }
        }

        Err(e) => {
            eprintln!("failed to read updated record: {:?}", e);
            return;
        }
    }


    // ------------------------------------------------------------
    // 17. Verify deletion survived
    // ------------------------------------------------------------

    let mut read_buffer = [0u8; RECORD_SIZE];

    match page.read_record(2, &mut read_buffer) {
        Ok(_) => {
            eprintln!("ERROR: deleted record survived");
            return;
        }

        Err(RecordError::RecordAbsent) => {
            println!(
                "read deleted persisted record 2: correctly returned RecordAbsent"
            );
        }

        Err(e) => {
            eprintln!("wrong error for deleted record: {:?}", e);
            return;
        }
    }


    println!();
    println!("=== PERSISTENCE TEST PASSED ===");
}
