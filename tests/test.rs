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

fn recover_test(){
    let mut index=match bootup(){
        Ok(i)=>i,
        Err(e)=>panic!("Bootup error : {:?}",e),
    };
    let rec=[0u8;7000];
    let _=index.write_record(&rec,rec.len());
    let _=index.shutdown();
    let _ =index.wal.add_log(TaskType::Delete,1,1,None);
    let mut index=match bootup(){
        Ok(i)=>i,
        Err(e)=>panic!("Bootup error : {:?}",e),
    };
    let _=match index.get_record(1){
        Ok(v)=>{
            println!("Retrieved record size {}",v.len());
        },
        Err(DbError::RecordAbsent)=>println!("Deleted"),
        Err(e)=>panic!("Retrieval error : {:?}",e),
    };
    println!("Test passed");
    let _=index.shutdown();
}
