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
    data_size       :   u16,
}

//data size comes before table name

impl Log{
    fn serialise(&self,row:Option<&[u8]>)->Result<Vec<u8>,DbError>{
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
        let dsize_bytes=self.data_size.to_le_bytes();
        let mut offset:usize=16;
        buf[offset..offset+2].copy_from_slice(&dsize_bytes);
        offset+=2;
        buf[offset..offset+self.table_name.len()].copy_from_slice(&tname_bytes);
        offset+=self.table_name.len();
        if let Some(row_buf)=row{
            buf[offset..self.log_size as usize].copy_from_slice(&row_buf);
        }
        Ok(buf)
    }

    fn deserialise(wal : &File,offset:u64)->Result<(Self,Option<Vec<u8>>),DbError>{
        let mut buf_size=[0u8;2];
        wal.read_at(&mut buf_size,offset).map_err(|_| DbError::FileError)?;
        let size:u16=u16::from_le_bytes(buf_size[0..2].try_into().map_err(|_| DbError::CorruptedWAL)?);
        if size<18{
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
        let mut offset:usize=16;
        let data_size=u16::from_le_bytes(log_buffer[offset..offset+2].try_into().map_err(|_| DbError::CorruptedDataError)?);
        offset+=2;
        if data_size>size-18{
            return Err(DbError::CorruptedWAL);
        } 
        let mut tname_buf:Vec<u8>=vec![0u8;size as usize-18-data_size as usize];
        let len=tname_buf.len();
        tname_buf.copy_from_slice(&log_buffer[offset..offset+len]);
        let table_name=String::from_utf8(tname_buf).map_err(|_| DbError::CorruptedWAL)?;
        offset+=table_name.len();
        let mut data:Option<Vec<u8>>=None;
        if data_size!=0{
            let mut row:Vec<u8>=vec![0u8;data_size as usize];
            row.copy_from_slice(&log_buffer[offset..offset+data_size as usize]);
            data=Some(row);
        }
        Ok((
            Self{
                log_size:size,
                lsn,
                task_type,
                record_id,
                table_name,
                data_size,
            },data
        ))
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
        self.file_size=10;
        let len_buf=self.length.to_le_bytes();
        self.wal.write_all_at(&len_buf,8).map_err(|_| DbError::FileError)?;
        self.wal.sync_data().map_err(|_| DbError::FileError)?;
        Ok(())
    }

    pub fn add_log(&mut self,task_type:TaskType,record_id:u32,table_name:&String,row:Option<&[u8]>)->Result<(),DbError>{
        let table_name=table_name.clone();
        let data_size=match row{
            Some(t)=>t.len() as u16,
            None=>0,
        };
        let entry:Log=match task_type{
            TaskType::Delete=>{
                let lsn=self.last_lsn+1;
                let log_size=18+table_name.len() as u16+data_size;
                let entry=Log{
                    log_size,
                    lsn,
                    task_type,
                    record_id,
                    table_name,
                    data_size,
                };
                entry
            },
            TaskType::Insert=>{
                let lsn:u64=self.last_lsn+1;
                let log_size:u16=18+table_name.len() as u16+data_size;
                let entry=Log{
                    log_size,
                    lsn,
                    task_type,
                    record_id,
                    table_name,
                    data_size,
                };
                entry
            },
        };
        let buf:Vec<u8>=entry.serialise(row)?;
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

    pub fn get_log_any(&self,iterator:Option<u64>)->Result<Option<(Log,Option<Vec<u8>>,u64)>,DbError>{
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
        let (log,data)=Log::deserialise(&self.wal,offset)?;
        let new_offset=offset+log.log_size as u64;
        Ok(Some((log,data,new_offset)))
    }

}
