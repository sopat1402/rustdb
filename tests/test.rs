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

    println!("Non persistent retrieval of record {id}");
    let rec=match index.get_record(id){
        Ok(v)=>v,
        Err(e)=>panic!("Retrieval failure : {:?}",e),
    };
    let s=String::from_utf8(rec).unwrap();
    println!("  - Successfully retrieved record {id} : {s}");

    println!("Updating record {id} to Bye");
    match index.update_record(id,b"Bye",3){
        Ok(_)=>println!("   - Successfully updated record {id}"),
        Err(e)=>panic!("Update record error : {:?}",e),
    };

    println!("Post update retrieval and check of record {id}");
    let rec=match index.get_record(id){
        Ok(v)=>v,
        Err(e)=>panic!("Retrieval failure : {:?}",e),
    };
    let s=String::from_utf8(rec).unwrap();
    println!("Retrieved record {id} : {s}");
    if s=="Bye"{
        println!("      - Retrieved data matches");
    }else{
        println!("      - Wrong record data retrieved");
        return;
    }

    println!("Persistence test of the record {id}");
    println!("Initiating shutdown...");
    match index.shutdown(){
        Ok(_)=>println!("   - Done!"),
        Err(e)=>panic!("Shutdown error : {:?}",e),
    };
    let mut index=match bootup(){
        Ok(idx)=>idx,
        Err(e)=>panic!("Bootup error : {:?}",e),
    };
    println!("Testing data persistence and correctness of record {id}");
    let rec=match index.get_record(id){
        Ok(v)=>v,
        Err(e)=>panic!("Retrieval failure : {:?}",e),
    };
    let s=String::from_utf8(rec).unwrap();
    println!("Retrieved record {id} : {s}");
    if s=="Bye"{
        println!("      - Retrieved data matches");
    }else{
        println!("      - Wrong record data retrieved");
        return;
    }

    println!("Deletion test of record {id}");
    match index.delete_record(id){
        Ok(_)=>println!("       - Index claims successful deletion"),
        Err(e)=>panic!("Deletion error : {:?}",e),
    };
    println!("Testing successful deletion");
    let _=match index.get_record(id){
        Ok(v)=>{
            let s=String::from_utf8(v).unwrap();
            println!("      - Did not delete the record, returned : {s}");
        },
        Err(DbError::RecordAbsent)=>println!("      - Successfully deleted record"),
        Err(e)=>panic!("Retrieval error : {:?}",e),
    };

    println!("WAL reconstruction test");
    println!("Initiating shutdown...");
    match index.shutdown(){
        Ok(_)=>println!("   - Done!"),
        Err(e)=>panic!("Shutdown error : {:?}",e),
    };
    println!("Writing a new record to only the WAL");
    let v=b"World";
    match index.wal.add_log(TaskType::Write,1,2,Some(v)){
        Ok(_)=>println!("   - Successfully wrote record to WAL"),
        Err(e)=>panic!("WAL write failure : {:?}",e),
    };
    println!("Rebooting the index");
    let mut index=match bootup(){
        Ok(idx)=>idx,
        Err(e)=>panic!("Bootup error : {:?}",e),
    };
    println!("Testing if record 2 exists");
    let rec=match index.get_record(2){
        Ok(v)=>v,
        Err(e)=>panic!("Retrieval error : {:?}",e),
    };
    println!("  - Index claims record exists");
    let s=String::from_utf8(rec).unwrap();
    println!("Record returned : {s}");
    if s=="World"{
        println!("  - Record was correct");
        println!("  - WAL was successfully used");
    }else{
        panic!("Record was incorrect");
    }

    println!("Test passed");

    println!("Initiating shutdown...");
    match index.shutdown(){
        Ok(_)=>println!("   - Done!"),
        Err(e)=>panic!("Shutdown error : {:?}",e),
    };
}

#[test]
fn wal_update_delete_recovery_test(){
    println!("Booting database");
    let mut index=match bootup(){
        Ok(idx)=>idx,
        Err(e)=>panic!("Bootup error : {:?}",e),
    };

    println!("Writing initial records");
    let update_id=match index.write_record(b"Hello",5){
        Ok(v)=>{
            println!("  - Wrote record {v} for WAL update test");
            v
        },
        Err(e)=>panic!("Write error : {:?}",e),
    };

    let delete_id=match index.write_record(b"World",5){
        Ok(v)=>{
            println!("  - Wrote record {v} for WAL delete test");
            v
        },
        Err(e)=>panic!("Write error : {:?}",e),
    };

    println!("Initiating clean shutdown...");
    match index.shutdown(){
        Ok(_)=>println!("  - Done!"),
        Err(e)=>panic!("Shutdown error : {:?}",e),
    };

    println!("Writing WAL-only update entry");
    match index.wal.add_log(
        TaskType::Update,
        1,
        update_id,
        Some(b"Updated")
    ){
        Ok(_)=>println!("  - Successfully wrote WAL update"),
        Err(e)=>panic!("WAL update write failure : {:?}",e),
    };
    println!("Writing WAL-only delete entry");
    match index.wal.add_log(
        TaskType::Delete,
        1,
        delete_id,
        None
    ){
        Ok(_)=>println!("  - Successfully wrote WAL delete"),
        Err(e)=>panic!("WAL delete write failure : {:?}",e),
    };

    println!("Rebooting database for WAL recovery");
    let mut index=match bootup(){
        Ok(idx)=>idx,
        Err(e)=>panic!("Recovery bootup error : {:?}",e),
    };

    println!("Testing WAL-recovered update of record {update_id}");
    let rec=match index.get_record(update_id){
        Ok(v)=>v,
        Err(e)=>panic!("Updated record retrieval failure : {:?}",e),
    };

    let s=String::from_utf8(rec).unwrap();
    println!("  - Record returned : {s}");

    if s=="Updated"{
        println!("  - WAL update recovery successful");
    }else{
        panic!("WAL update recovery returned incorrect data");
    }

    println!("Testing WAL-recovered deletion of record {delete_id}");
    match index.get_record(delete_id){
        Err(DbError::RecordAbsent)=>{
            println!("  - WAL delete recovery successful");
        },
        Ok(v)=>{
            let s=String::from_utf8(v).unwrap();
            panic!("Deleted record still exists : {s}");
        },
        Err(e)=>panic!("Deletion recovery retrieval error : {:?}",e),
    };

    println!("Update and delete WAL recovery test passed");

    println!("Initiating shutdown...");
    match index.shutdown(){
        Ok(_)=>println!("  - Done!"),
        Err(e)=>panic!("Shutdown error : {:?}",e),
    };
}
