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

/*#[test]
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
}*/

#[test]
fn wal_recovery_space_over_test(){
    println!("Booting database");
    let mut index=match bootup(){
        Ok(idx)=>idx,
        Err(e)=>panic!("Bootup error : {:?}",e),
    };

    /*
        FILL PAGE FOR WRITE SPACEOVER TEST

        PAGE_SIZE is 8192, so we write sufficiently large records
        to make the first page unable to accept another record.
    */

    println!("Filling page for write recovery SpaceOver test");

    let large=vec![b'A'; 7000];

    let id1=match index.write_record(&large,large.len()){
        Ok(id)=>{
            println!("  - Wrote large record {id}");
            id
        },
        Err(e)=>panic!("Initial large write failed : {:?}",e),
    };

    println!("Shutting down before WAL-only write");
    match index.shutdown(){
        Ok(_)=>println!("  - Done!"),
        Err(e)=>panic!("Shutdown error : {:?}",e),
    };

    /*
        The database is clean now.

        We directly add a write log targeting page 1.
        The record should not fit there, so recovery must hit
        the Write -> SpaceOver branch.
    */

    println!("Adding WAL-only write that should overflow page 1");

    match index.wal.add_log(
        TaskType::Write,
        1,
        id1+1,
        Some(b"Recovered Write")
    ){
        Ok(_)=>println!("  - WAL-only write added"),
        Err(e)=>panic!("WAL write failed : {:?}",e),
    };

    println!("Rebooting for write recovery");

    let mut index=match bootup(){
        Ok(idx)=>idx,
        Err(e)=>panic!("Recovery bootup error : {:?}",e),
    };

    println!("Checking recovered overflow write");

    let record=match index.get_record(id1+1){
        Ok(v)=>v,
        Err(e)=>panic!("Recovered write missing : {:?}",e),
    };

    let s=String::from_utf8(record).unwrap();

    if s=="Recovered Write"{
        println!("  - Write SpaceOver recovery passed");
    }else{
        panic!("Incorrect recovered write : {s}");
    }

    /*
        UPDATE SPACEOVER TEST

        First write a small record to a page.
        Then add a WAL-only update with a larger value which should
        no longer fit on that record's original page.
    */

    println!("Setting up update recovery SpaceOver test");

    let small_id=match index.write_record(b"Small",5){
        Ok(id)=>{
            println!("  - Wrote record {id}");
            id
        },
        Err(e)=>panic!("Small write failed : {:?}",e),
    };

    println!("Shutting down before WAL-only oversized update");

    match index.shutdown(){
        Ok(_)=>println!("  - Done!"),
        Err(e)=>panic!("Shutdown error : {:?}",e),
    };

    let larger_update=vec![b'B'; 7000];

    println!("Adding WAL-only update expected to overflow original page");

    match index.wal.add_log(
        TaskType::Update,
        1,
        small_id,
        Some(&larger_update)
    ){
        Ok(_)=>println!("  - WAL-only update added"),
        Err(e)=>panic!("WAL update failed : {:?}",e),
    };

    println!("Rebooting for update recovery");

    let mut index=match bootup(){
        Ok(idx)=>idx,
        Err(e)=>panic!("Recovery bootup error : {:?}",e),
    };

    println!("Checking recovered oversized update");

    let record=match index.get_record(small_id){
        Ok(v)=>v,
        Err(e)=>panic!("Recovered update missing : {:?}",e),
    };

    if record==larger_update{
        println!("  - Update SpaceOver recovery passed");
    }else{
        panic!("Recovered update data was incorrect");
    }

    println!("Both recovery SpaceOver branches passed");

    println!("Initiating final shutdown");

    match index.shutdown(){
        Ok(_)=>println!("  - Done!"),
        Err(e)=>panic!("Shutdown error : {:?}",e),
    };
}
