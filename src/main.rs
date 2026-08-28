//Code by Sohum Pathak
//sohum.pathak@protonmail.com

use std::fs::File;
use rustdb::db_errors::DbError;
use rustdb::index::Index;
use rustdb::page::DatabaseFile;


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

fn main(){
    let mut index:Index=match bootup(){
        Ok(idx)=>{idx},
        Err(e)=>panic!("Bootup error : {:?}",e),
    };
    match index.shutdown(){
        Ok(_)=>{},
        Err(e)=>panic!("Shutdown error : {:?}",e),
    };
}
