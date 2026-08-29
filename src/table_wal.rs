use std::os::unix::prelude::FileExt;
use std::fs::File;
use crate::db_errors::DbError;
use std::vec::Vec;

pub struct TableWAL{
    pub length      :   u16,
    wal             :   File,
    pub last_lsn    :   u64,
    pub file_size       :   u64,
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

impl Log{
    fn serialise(&self)->Result<Vec<u8>,DbError>{
        let mut buf:Vec<u8>=vec![0u8;self.log_size as usize];
        let log_size_bytes=self.log_size.to_le_bytes();
        let lsn_bytes=self.lsn.to_le_bytes();
        let task:u16=match self.task_type{
            TaskType::Insert=>0,
            TaskType::Delete=>1,
        };
        let task_bytes=task.to_le_bytes();
        buf[0..2].copy_from_slice(&log_size_bytes);
        buf[2..10].copy_from_slice(&lsn_bytes);
        buf[10..12].copy_from_slice(&task_bytes);
        let record_id_bytes=self.record_id.to_le_bytes();
        buf[12..16].copy_from_slice(&record_id_bytes);
        let tname_bytes=self.table_name.as_bytes();
        buf[16..self.log_size as usize].copy_from_slice(&tname_bytes);
        Ok(buf)
    }

    fn deserialise(wal : &File,offset:u64)->Result<Self,DbError>{
        let mut buf_size=[0u8;2];
        wal.read_at(&mut buf_size,offset).map_err(|_| DbError::FileError)?;
        let size:u16=u16::from_le_bytes(buf_size[0..2].try_into().map_err(|_| DbError::CorruptedWAL)?);
        if size<16{
            return Err(DbError::CorruptedWAL);
        }
        let mut log_buffer:Vec<u8>=vec![0u8;size as usize];
        if wal.read_at(&mut log_buffer,offset).map_err(|_| DbError::FileError)? !=size as usize{
            return Err(DbError::CorruptedWAL);
        }
        let lsn=u64::from_le_bytes(log_buffer[2..10].try_into().map_err(|_| DbError::CorruptedWAL)?);
        let task=u16::from_le_bytes(log_buffer[10..12].try_into().map_err(|_| DbError::CorruptedWAL)?);
        let task_type=match task{
            0=>TaskType::Insert,
            1=>TaskType::Delete,
            _=>return Err(DbError::CorruptedWAL),
        };
        let record_id=u32::from_le_bytes(log_buffer[12..16].try_into().map_err(|_| DbError::CorruptedWAL)?);
        let mut data:Vec<u8>=vec![0u8;size as usize-16];
        data.copy_from_slice(&log_buffer[16..size as usize]);
        let table_name=String::from_utf8(data).map_err(|_| DbError::CorruptedWAL)?;
        Ok(
            Self{
                log_size:size,
                lsn,
                task_type,
                record_id,
                table_name,
            }
        )
    }
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
            file_size:size,
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

    pub fn add_log(&mut self,task_type:TaskType,record_id:u32,table_name:&String)->Result<(),DbError>{
        let table_name=table_name.clone();
        let entry:Log=match task_type{
            TaskType::Delete=>{
                let lsn=self.last_lsn+1;
                let log_size=16+table_name.len() as u16;
                let entry=Log{
                    log_size,
                    lsn,
                    task_type,
                    record_id,
                    table_name,
                };
                entry
            },
            TaskType::Insert=>{
                let lsn:u64=self.last_lsn+1;
                let log_size:u16=16+table_name.len() as u16;
                let entry=Log{
                    log_size,
                    lsn,
                    task_type,
                    record_id,
                    table_name,
                };
                entry
            },
        };
        let buf:Vec<u8>=entry.serialise()?;
        self.wal.write_all_at(&buf,self.file_size).map_err(|_| DbError::FileError)?;
        self.length+=1;
        self.last_lsn+=1;
        let len_buf=self.length.to_le_bytes();
        let lsn_buf=self.last_lsn.to_le_bytes();
        self.wal.write_all_at(&len_buf,8).map_err(|_| DbError::FileError)?;
        self.wal.write_all_at(&lsn_buf,0).map_err(|_| DbError::FileError)?;
        self.file_size+=entry.log_size as u64;
        self.wal.sync_data().map_err(|_| DbError::FileError)?;
        Ok(())
    }

    pub fn get_log_any(&self,iterator:Option<u64>)->Result<Option<(Log,u64)>,DbError>{
        let offset:u64=match iterator{
            Some(v)=>{
                if v<10 {
                    10
                }
                else{
                    v
                }
            },
            None=>10,
        };
        if offset>=self.file_size {
            return Ok(None);
        }
        let log=Log::deserialise(&self.wal,offset)?;
        let new_offset=offset+log.log_size as u64;
        Ok(Some((log,new_offset)))
    }

}
