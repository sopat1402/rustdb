use std::fs::File;

use rustdb::db_errors::DbError;
use rustdb::index::Index;
use rustdb::page::DatabaseFile;
use rustdb::slotted_page::RECORD_SIZE;

fn make_record(id: u32, text: &[u8]) -> Result<([u8; RECORD_SIZE], usize),DbError> {
    let mut record = [0u8; RECORD_SIZE];

    record[0..4].copy_from_slice(&id.to_le_bytes());

    let end = 4 + text.len();
    if end>RECORD_SIZE {
        return Err(DbError::SpaceOver);
    }
    record[4..end].copy_from_slice(text);

    Ok((record, end))
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

#[test]
fn test(){
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
    let (record1,size1)=match make_record(1,b"This is record 1"){
        Ok(v)=>v,
        Err(_)=>return,
    };
    match index.write_record(&record1,size1){
        Ok(_)=>println!("Wrote record 1"),
        Err(e)=>{
            eprintln!("Failed to write record 1 {:?}",e);
            return;
        },
    };
    let (record2, size2) =match make_record(2, b"This is record 2"){
        Ok(v)=>v,
        Err(_)=>return,
    };
    match index.write_record(&record2, size2) {
        Ok(_) => println!("write record 2"),
        Err(e) => {
            eprintln!("Failed to write record 2: {:?}", e);
            return;
        }
    }
    let (record3, size3) = match make_record(3, b"This is record 3"){
        Ok(v)=>v,
        Err(_)=>return,
    };
    match index.write_record(&record3, size3) {
        Ok(_) => println!("Wrote record 3"),
        Err(e) => {
            eprintln!("Failed to write record 3: {:?}", e);
            return;
        }
    }
    let mut read_buf=[0u8;RECORD_SIZE];
    match index.get_record(1,&mut read_buf){
        Ok(size)=>{
            println!("Got record 1 {:?}",&read_buf[4..size]);
        },
        Err(e)=>{
            println!("Failed to get record 1 {:?}",e);
        }
    };
    read_buf=[0u8;RECORD_SIZE];
    match index.get_record(2,&mut read_buf){
        Ok(size)=>{
            println!("Got record 2 {:?}",&read_buf[4..size]);
        },
        Err(e)=>{
            println!("Failed to get record 2 {:?}",e);
        }
    };
    read_buf=[0u8;RECORD_SIZE];
    match index.get_record(3,&mut read_buf){
        Ok(size)=>{
            println!("Got record 3 {:?}",&read_buf[4..size]);
        },
        Err(e)=>{
            println!("Failed to get record 3 {:?}",e);
        }
    };
    read_buf=[0u8;RECORD_SIZE];
    match index.pool.evict_all() {
        Ok(()) => println!("Flushed buffer pool."),
        Err(e) => {
            eprintln!("Failed to flush buffer pool: {:?}", e);
            return;
        }
    }
    match index.tree.serialise(&mut index.pool.db_file) {
        Ok(()) => println!("Serialised B+ tree"),
        Err(e) => {
            eprintln!("Failed to serialise B+ tree: {:?}", e);
            return;
        }
    }
    drop(index);
    println!("Dropped entire index.");
    let db = match open_database() {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Failed to reopen database files: {:?}", e);
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
    match index.get_record(1,&mut read_buf){
        Ok(size)=>{
            println!("Got record 1 after restart{:?}",&read_buf[4..size]);
        },
        Err(e)=>{
            println!("Failed to get record 1 {:?}",e);
        }
    };
    read_buf=[0u8;RECORD_SIZE];
    match index.get_record(2,&mut read_buf){
        Ok(size)=>{
            println!("Got record 2 after restart{:?}",&read_buf[4..size]);
        },
        Err(e)=>{
            println!("Failed to get record 2 {:?}",e);
        }
    };
    read_buf=[0u8;RECORD_SIZE];
    match index.get_record(3,&mut read_buf){
        Ok(size)=>{
            println!("Got record 3 after restart{:?}",&read_buf[4..size]);
        },
        Err(e)=>{
            println!("Failed to get record 3 {:?}",e);
        }
    };
}
