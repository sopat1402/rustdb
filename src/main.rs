mod page;
mod lru_cache;
mod slotted_page;
mod buffer_pool;

use std::fs::File;

use page::{DatabaseFile, PAGE_SIZE, PageHeader, PageType};
use slotted_page::{Page, RECORD_SIZE, RecordError};
use buffer_pool::BufferPool;

fn main() {
    println!("=== BUFFER POOL TEST ===");

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

    // ------------------------------------------------------------
    // 1. Create two pages directly on disk.
    //
    // Buffer pool capacity = 1, so loading page 1 after page 0
    // will evict page 0 and flush it.
    // ------------------------------------------------------------

    let mut buffer_pool = BufferPool::new(1, db);

    let page0 = match buffer_pool.allocate_page() {
        Ok(id) => {
            println!("allocated page {id}");
            id
        }
        Err(e) => {
            eprintln!("failed to allocate page 0: {e}");
            return;
        }
    };

    let page1 = match buffer_pool.allocate_page() {
        Ok(id) => {
            println!("allocated page {id}");
            id
        }
        Err(e) => {
            eprintln!("failed to allocate page 1: {e}");
            return;
        }
    };

    // ------------------------------------------------------------
    // 2. Create page 0 and write it to disk.
    //
    // This establishes known persisted state.
    // ------------------------------------------------------------

    let header = PageHeader::new(page0, PageType::Data);
    let buffer = [0u8; PAGE_SIZE];

    let mut page = Page::new(header, buffer);

    let mut record = [0u8; RECORD_SIZE];
    record[0..2].copy_from_slice(&1u16.to_le_bytes());
    record[2..7].copy_from_slice(b"hello");

    match page.write_record(1, &record, 7) {
        Ok(_) => println!("created page 0 with record: hello"),
        Err(e) => {
            eprintln!("failed to write record: {:?}", e);
            return;
        }
    }

    page.header.serialise(&mut page.buffer);

    if let Err(e) = page.flush(&buffer_pool.db_file) {
        eprintln!("failed to flush initial page: {:?}", e);
        return;
    }

    println!("page 0 persisted to disk");

    // ------------------------------------------------------------
    // 3. Load page 0 through the buffer pool.
    // ------------------------------------------------------------

    match buffer_pool.get_page(page0) {
        Ok(page) => {
            let mut read_buffer = [0u8; RECORD_SIZE];

            match page.read_record(1, &mut read_buffer) {
                Ok(size) => {
                    assert_eq!(&read_buffer[2..size], b"hello");
                    println!("get_page(page 0): loaded persisted data");
                }
                Err(e) => {
                    eprintln!("failed to read cached page: {:?}", e);
                    return;
                }
            }
        }

        Err(e) => {
            eprintln!("failed to get page 0: {:?}", e);
            return;
        }
    }

    // ------------------------------------------------------------
    // 4. Modify page 0 THROUGH THE BUFFER POOL.
    //
    // This should modify only the in-memory copy.
    // It should NOT hit disk yet.
    // ------------------------------------------------------------

    {
        let page = match buffer_pool.get_page_mut(page0) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("failed to get mutable page 0: {:?}", e);
                return;
            }
        };

        let mut updated = [0u8; RECORD_SIZE];
        updated[0..2].copy_from_slice(&1u16.to_le_bytes());
        updated[2..9].copy_from_slice(b"updated");

        match page.update_record(1, &updated, 9) {
            Ok(_) => println!("modified page 0 in buffer pool"),
            Err(e) => {
                eprintln!("failed to update page: {:?}", e);
                return;
            }
        }

        page.header.serialise(&mut page.buffer);
    }

    // ------------------------------------------------------------
    // 5. Verify that the BUFFER contains the update.
    // ------------------------------------------------------------

    {
        let page = match buffer_pool.get_page(page0) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("failed to get cached page 0: {:?}", e);
                return;
            }
        };

        let mut read_buffer = [0u8; RECORD_SIZE];

        match page.read_record(1, &mut read_buffer) {
            Ok(size) => {
                assert_eq!(&read_buffer[2..size], b"updated");
                println!("cached page 0 contains: updated");
            }
            Err(e) => {
                eprintln!("failed to read cached update: {:?}", e);
                return;
            }
        }
    }

    // ------------------------------------------------------------
    // 6. Load page 1.
    //
    // Capacity is 1.
    //
    // Therefore page 0 must be evicted.
    // Your LRU's pop_tail() should flush page 0.
    // ------------------------------------------------------------

    match buffer_pool.get_page(page1) {
        Ok(_) => {
            println!("loaded page 1");
            println!("page 0 was evicted from the buffer pool");
        }

        Err(e) => {
            eprintln!("failed to load page 1: {:?}", e);
            return;
        }
    }

    // ------------------------------------------------------------
    // 7. Get page 0 again.
    //
    // It is no longer cached, so this MUST load it from disk.
    //
    // If eviction flushed correctly, the update should survive.
    // ------------------------------------------------------------

    match buffer_pool.get_page(page0) {
        Ok(page) => {
            let mut read_buffer = [0u8; RECORD_SIZE];

            match page.read_record(1, &mut read_buffer) {
                Ok(size) => {
                    assert_eq!(&read_buffer[2..size], b"updated");

                    println!(
                        "reloaded page 0 from disk: update survived eviction"
                    );
                }

                Err(e) => {
                    eprintln!(
                        "page 0 was reloaded but update was lost: {:?}",
                        e
                    );
                    return;
                }
            }
        }

        Err(e) => {
            eprintln!("failed to reload page 0: {:?}", e);
            return;
        }
    }

    // ------------------------------------------------------------
    // 8. Explicit flush_page test.
    //
    // Modify page 0 again, then explicitly flush it.
    // ------------------------------------------------------------

    {
        let page = match buffer_pool.get_page_mut(page0) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("failed to get mutable page 0: {:?}", e);
                return;
            }
        };

        let mut updated = [0u8; RECORD_SIZE];
        updated[0..2].copy_from_slice(&1u16.to_le_bytes());
        updated[2..10].copy_from_slice(b"flushed!");

        match page.update_record(1, &updated, 10) {
            Ok(_) => println!("modified page 0 again"),
            Err(e) => {
                eprintln!("failed to update page: {:?}", e);
                return;
            }
        }

        page.header.serialise(&mut page.buffer);
    }

    match buffer_pool.flush_page(page0) {
        Ok(()) => println!("flush_page(page 0): OK"),
        Err(e) => {
            eprintln!("flush_page failed: {:?}", e);
            return;
        }
    }

    // ------------------------------------------------------------
    // 9. Get page 0 again.
    //
    // flush_page() removed it from the cache, so this loads
    // the persisted version.
    // ------------------------------------------------------------

    match buffer_pool.get_page(page0) {
        Ok(page) => {
            let mut read_buffer = [0u8; RECORD_SIZE];

            match page.read_record(1, &mut read_buffer) {
                Ok(size) => {
                    assert_eq!(&read_buffer[2..size], b"flushed!");
                    println!("reloaded page 0: explicit flush survived");
                }

                Err(e) => {
                    eprintln!("failed to read flushed page: {:?}", e);
                    return;
                }
            }
        }

        Err(e) => {
            eprintln!("failed to reload flushed page: {:?}", e);
            return;
        }
    }

    // ------------------------------------------------------------
    // 10. Flush everything remaining in the buffer pool.
    // ------------------------------------------------------------

    buffer_pool.flush_all();

    println!();
    println!("=== BUFFER POOL TEST PASSED ===");
}
