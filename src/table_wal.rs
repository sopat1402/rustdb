use std::os::unix::prelude::FileExt;
use std::fs::File;
use crate::db_errors::DbError;
use std::vec::Vec;

pub struct TableWAL{
    pub length      :   u16,
    wal             :   File,
    pub last_lsn    :   u64,
}

#[repr(u16)]
pub enum TaskType{
    Insert,
    Delete,
}

pub struct Log{
    pub log_size    :   u16,
    pub lsn         :   u64,
    pub task_type   :   TaskType,
    pub record_id   :   u32,
    pub table_name  :   String,
}

impl TableWAL{
    pub fn deserialise(wal:File)->Result<Self,DbError>{
        let mut size=wal.metadata().map_err(|_| DbError::FileError)?.len();
        let last_lsn:u64;
        if size==0{
            last_lsn=0;
        }
        else if size>=8{
            let mut buf=[0u8;8];
            match wal.read_at(&mut buf,0){
                Ok(_)=>{},
                Err(_)=>return Err(DbError::FileError),
            };
            last_lsn=u64::from_le_bytes(buf[0..8].try_into().unwrap());
        }
        else{
            return Err(DbError::CorruptedWAL);
        }
        let length:u16;
        if size==0{
            length=0;
        }
        else if size>=10{
            let mut buf=[0u8;2];
            match wal.read_at(&mut buf,8){
                Ok(_)=>{},
                Err(_)=>return Err(DbError::FileError),
            };
            length=u16::from_le_bytes(buf[0..2].try_into().unwrap());
        }
        else{
            return Err(DbError::CorruptedWAL);
        }
        if size==0{
            let lsn_buf=last_lsn.to_le_bytes();
            let len_buf=length.to_le_bytes();
            wal.write_all_at(&lsn_buf,0).map_err(|_| DbError::FileError)?;
            wal.write_all_at(&len_buf,8).map_err(|_| DbError::FileError)?;
            wal.sync_data().map_err(|_| DbError::FileError)?;
            size=10;
        }
        Ok(Self{
            length,
            wal,
            last_lsn,
        })
    }

    pub fn reset(&mut self)->Result<(),DbError>{
        self.wal.set_len(10).map_err(|_| DbError::FileError)?;
        self.length=0;
        let len_buf=self.length.to_le_bytes();
        self.wal.write_all_at(&len_buf,8).map_err(|_| DbError::FileError)?;
        self.wal.sync_data().map_err(|_| DbError::FileError)?;
        Ok(())
    }
}
