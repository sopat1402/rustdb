//Code by Sohum Pathak
//sohum.pathak@protonmail.com

use std::fs::{self, File};
use std::path::PathBuf;

use rustdb::db_errors::DbError;
use rustdb::index::Index;
use rustdb::page::{DatabaseFile, PageHeader, PageType, PAGE_SIZE};
use rustdb::slotted_page::Page;

fn record(id: u32, payload_len: usize, fill: u8) -> Vec<u8> {
    let mut value = vec![fill; payload_len + 4];
    value[..4].copy_from_slice(&id.to_le_bytes());
    value
}

fn database(name: &str) -> (DatabaseFile, Vec<PathBuf>) {
    let root = std::env::temp_dir().join(format!(
        "rustdb-{}-{}-{}",
        name,
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));

    let database_path = root.with_extension("db");
    let metadata_path = root.with_extension("page");
    let tree_path = root.with_extension("tree");

    let file = File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&database_path)
        .unwrap();

    let page_metadata = File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&metadata_path)
        .unwrap();

    let btree = File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&tree_path)
        .unwrap();

    let size = file.metadata().unwrap().len();

    (
        DatabaseFile {
            file,
            page_metadata,
            btree,
            size,
        },
        vec![database_path, metadata_path, tree_path],
    )
}

fn remove_database(paths: Vec<PathBuf>) {
    for path in paths {
        let _ = fs::remove_file(path);
    }
}

#[test]
fn compaction_removes_deleted_slots_and_preserves_live_records() {
    let mut page = Page::new(
        PageHeader::new(1, PageType::Data),
        [0u8; PAGE_SIZE],
    );

    let first = record(1, 2_900, b'a');
    let second = record(2, 2_900, b'b');
    let replacement = record(3, 3_400, b'c');

    page.write_record(1, &first, first.len()).unwrap();
    page.write_record(2, &second, second.len()).unwrap();
    page.delete_record(1).unwrap();

    // The hole is smaller than replacement, so write_record must
    // compact the page before inserting the replacement.
    page.write_record(3, &replacement, replacement.len())
        .unwrap();

    assert_eq!(page.read_record(2).unwrap(), second);
    assert_eq!(page.read_record(3).unwrap(), replacement);

    assert!(matches!(
        page.read_record(1),
        Err(DbError::RecordAbsent)
    ));

    assert!(page.trash.is_empty());
    assert_eq!(page.header.item_count, 2);
}

#[test]
fn index_update_replaces_record_in_place_when_new_value_fits() {
    let (db, paths) = database("update-in-place");
    let mut index = Index::new(db).unwrap();

    let original = record(1, 100, b'a');
    let updated = record(1, 40, b'z');

    index
        .write_record(&original, original.len())
        .unwrap();

    let page_before = index.tree.search(1).unwrap();

    assert_eq!(
        index
            .update_record(1, &updated, updated.len())
            .unwrap(),
        updated.len()
    );

    let page_after = index.tree.search(1).unwrap();

    assert_eq!(page_before, page_after);
    assert_eq!(index.get_record(1).unwrap(), updated);

    drop(index);
    remove_database(paths);
}

#[test]
fn index_update_grows_record_and_compacts_the_page() {
    let (db, paths) = database("update-grow");
    let mut index = Index::new(db).unwrap();

    let first = record(1, 120, b'a');
    let second = record(2, 120, b'b');
    let third = record(3, 120, b'c');
    let enlarged = record(1, 500, b'x');

    index.write_record(&first, first.len()).unwrap();
    index.write_record(&second, second.len()).unwrap();
    index.write_record(&third, third.len()).unwrap();

    let page_before = index.tree.search(1).unwrap();

    index
        .update_record(1, &enlarged, enlarged.len())
        .unwrap();

    assert_eq!(index.tree.search(1).unwrap(), page_before);
    assert_eq!(index.get_record(1).unwrap(), enlarged);
    assert_eq!(index.get_record(2).unwrap(), second);
    assert_eq!(index.get_record(3).unwrap(), third);

    let page = index.pool.get_page_mut(page_before).unwrap();

    assert!(page.trash.is_empty());
    assert_eq!(page.header.item_count, 3);

    drop(index);
    remove_database(paths);
}

#[test]
fn index_update_moves_record_to_another_page_when_page_is_full() {
    let (db, paths) = database("update-relocate");
    let mut index = Index::new(db).unwrap();

    let first = record(1, 4_000, b'a');
    let second = record(2, 3_200, b'b');
    let enlarged = record(1, 5_000, b'x');

    index.write_record(&first, first.len()).unwrap();
    index.write_record(&second, second.len()).unwrap();

    let old_page = index.tree.search(1).unwrap();

    index
        .update_record(1, &enlarged, enlarged.len())
        .unwrap();

    let new_page = index.tree.search(1).unwrap();

    assert_ne!(new_page, old_page);
    assert_eq!(index.get_record(1).unwrap(), enlarged);
    assert_eq!(index.get_record(2).unwrap(), second);

    assert!(matches!(
        index.get_record(99),
        Err(DbError::RecordAbsent)
    ));

    drop(index);
    remove_database(paths);
}
