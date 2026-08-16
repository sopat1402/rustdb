mod page;
mod slotted_page;
use std::fs::File;
use std::io::Seek;
use std::process;
use std::io::SeekFrom;
use page::DatabaseFile;
use page::{PageHeader, PageType, PAGE_SIZE};
use slotted_page::{Page, RECORD_SIZE, RecordError};

fn main() {
    println!("=== PAGE TEST ===");

    // ------------------------------------------------------------
    // 1. Create a fresh page
    // ------------------------------------------------------------

    let header = PageHeader::new(0, PageType::Data);
    let buffer = [0u8; PAGE_SIZE];

    let mut page = Page::new(header, buffer);

    println!("Created page {}", page.header.page_id);


    // ------------------------------------------------------------
    // 2. Write record #1
    //
    // First 2 bytes = record ID
    // Remaining bytes = actual record data
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


    // ------------------------------------------------------------
    // 3. Write record #2
    // ------------------------------------------------------------

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
    // 4. Read record #1
    // ------------------------------------------------------------

    let mut read_buffer = [0u8; RECORD_SIZE];

    match page.read_record(1, &mut read_buffer) {
        Ok(size) => {
            println!(
                "read record 1: OK ({size} bytes): {:?}",
                &read_buffer[2..size]
            );
        }

        Err(e) => {
            eprintln!("read record 1 failed: {:?}", e);
            return;
        }
    }


    // ------------------------------------------------------------
    // 5. Read record #2
    // ------------------------------------------------------------

    read_buffer = [0u8; RECORD_SIZE];

    match page.read_record(2, &mut read_buffer) {
        Ok(size) => {
            println!(
                "read record 2: OK ({size} bytes): {:?}",
                &read_buffer[2..size]
            );
        }

        Err(e) => {
            eprintln!("read record 2 failed: {:?}", e);
            return;
        }
    }


    // ------------------------------------------------------------
    // 6. Try reading nonexistent record
    // ------------------------------------------------------------

    read_buffer = [0u8; RECORD_SIZE];

    match page.read_record(999, &mut read_buffer) {
        Ok(_) => {
            eprintln!("ERROR: nonexistent record was found");
            return;
        }

        Err(RecordError::RecordAbsent) => {
            println!("read nonexistent record: correctly returned RecordAbsent");
        }

        Err(e) => {
            eprintln!(
                "read nonexistent record: wrong error {:?}",
                e
            );
            return;
        }
    }


    // ------------------------------------------------------------
    // 7. Update record #1
    // ------------------------------------------------------------

    let mut updated = [0u8; RECORD_SIZE];

    updated[0..2].copy_from_slice(&1u16.to_le_bytes());
    updated[2..9].copy_from_slice(b"updated");

    match page.update_record(1, &updated, 9) {
        Ok(size) => println!("update record 1: OK ({size} bytes)"),
        Err(e) => {
            eprintln!("update record 1 failed: {:?}", e);
            return;
        }
    }


    // ------------------------------------------------------------
    // 8. Read updated record
    // ------------------------------------------------------------

    read_buffer = [0u8; RECORD_SIZE];

    match page.read_record(1, &mut read_buffer) {
        Ok(size) => {
            println!(
                "read updated record 1: OK ({size} bytes): {:?}",
                &read_buffer[2..size]
            );
        }

        Err(e) => {
            eprintln!("read updated record failed: {:?}", e);
            return;
        }
    }


    // ------------------------------------------------------------
    // 9. Delete record #2
    // ------------------------------------------------------------

    match page.delete_record(2) {
        Ok(()) => println!("delete record 2: OK"),
        Err(e) => {
            eprintln!("delete record 2 failed: {:?}", e);
            return;
        }
    }


    // ------------------------------------------------------------
    // 10. Verify deleted record cannot be read
    // ------------------------------------------------------------

    read_buffer = [0u8; RECORD_SIZE];

    match page.read_record(2, &mut read_buffer) {
        Ok(_) => {
            eprintln!("ERROR: deleted record can still be read");
            return;
        }

        Err(RecordError::RecordAbsent) => {
            println!("read deleted record: correctly returned RecordAbsent");
        }

        Err(e) => {
            eprintln!(
                "read deleted record: wrong error {:?}",
                e
            );
            return;
        }
    }


    // ------------------------------------------------------------
    // 11. Reuse deleted slot
    // ------------------------------------------------------------

    let mut record3 = [0u8; RECORD_SIZE];

    record3[0..2].copy_from_slice(&3u16.to_le_bytes());
    record3[2..8].copy_from_slice(b"reused");

    match page.write_record(3, &record3, 8) {
        Ok(size) => {
            println!(
                "write record 3 into deleted slot: OK ({size} bytes)"
            );
        }

        Err(e) => {
            eprintln!("write record 3 failed: {:?}", e);
            return;
        }
    }


    // ------------------------------------------------------------
    // 12. Verify reused record
    // ------------------------------------------------------------

    read_buffer = [0u8; RECORD_SIZE];

    match page.read_record(3, &mut read_buffer) {
        Ok(size) => {
            println!(
                "read reused record 3: OK ({size} bytes): {:?}",
                &read_buffer[2..size]
            );
        }

        Err(e) => {
            eprintln!("read reused record failed: {:?}", e);
            return;
        }
    }


    println!();
    println!("=== PAGE TEST PASSED ===");
}
