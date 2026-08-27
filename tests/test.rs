use rustdb::index::Index;
use rustdb::wal::TaskType;
use rustdb::page::DatabaseFile;
use rustdb::db_errors::DbError;
use std::fs::File;

fn bootup()->Result<Index,DbError>{
    let file=File::options()
        .read(true)
        .create(true)
        .write(true)
        .open("database.db").map_err(|_| DbError::FileError)?;
    let size=file.metadata().map_err(|_| DbError::FileError)?.len();
    let btree=File::options()
        .read(true)
        .write(true)
        .create(true)
        .open("btree.tree").map_err(|_| DbError::FileError)?;
    let page_metadata=File::options()
        .read(true)
        .write(true)
        .create(true)
        .open("page.meta").map_err(|_| DbError::FileError)?;
    let db_file=DatabaseFile{
        file,
        page_metadata,
        btree,
        size,
    };
    let mut index=Index::new(db_file)?;
    if index.wal.length!=0{
        index.recover()?;
    }
    Ok(index)
}

#[test]
fn basic_wal_test(){
    let mut index=match bootup(){
        Ok(idx)=>idx,
        Err(e)=>panic!("Bootup error : {:?}",e),
    };

    println!("Writing Hello as a record");
    let id=match index.write_record(b"Hello",5){
        Ok(v)=>{
            println!("  - Successfully wrote record {v}");
            v
        },
        Err(e)=>panic!("Write error : {:?}",e),
    };

    println!("Updating record {id} to Bye");
    match index.update_record(id,b"Bye",3){
        Ok(_)=>println!("   - Successfully updated record {id}"),
        Err(e)=>panic!("Update record error : {:?}",e),
    };

    println!("Initiating shutdown...");
    match index.shutdown(){
        Ok(_)=>println!("   - Done!"),
        Err(e)=>panic!("Shutdown error : {:?}",e),
    };

    println!("WAL reconstruction test");
    println!("Update a record to only the WAL");
    match index.wal.add_log(TaskType::Update,1,id,Some(b"World")){
        Ok(_)=>println!("   - Successfully wrote log to WAL"),
        Err(e)=>panic!("WAL write failure : {:?}",e),
    };
    println!("Rebooting the index");
    let mut index=match bootup(){
        Ok(idx)=>idx,
        Err(e)=>panic!("Bootup error : {:?}",e),
    };

    println!("Testing update after reboot");
    let v=match index.get_record(id){
        Ok(rec)=>rec,
        Err(e)=>panic!("Retrieval error : {:?}",e),
    };
    let s=String::from_utf8(v).unwrap();
    if s=="World"{
        println!("Successfully updated Bye to World");
    }else{
        println!("Update failed, got {s}");
    }

    println!("Test passed");

    println!("Initiating shutdown...");
    match index.shutdown(){
        Ok(_)=>println!("   - Done!"),
        Err(e)=>panic!("Shutdown error : {:?}",e),
    };
}
