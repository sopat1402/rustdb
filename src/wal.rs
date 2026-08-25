use std::os::unix::prelude::FileExt;
use std::fs::File;
use crate::db_errors::DbError;
use std::vec::Vec;

pub struct WAL{
    length      :   u16,
    wal         :   File,
    last_lsn    :   u64,
    file_size   :   u64,
}

#[repr(u16)]
pub enum TaskType{
    Write,
    Update,
    Delete,
}

pub struct Log{
    pub log_size    :   u16,
    pub lsn         :   u64,
    pub task_type   :   TaskType,
    pub page_id     :   u64,
    pub record_id   :   u32,
}

impl Log{
    fn serialise(&self,data:Option<&Vec<u8>>)->Result<Vec<u8>,DbError>{
        let mut buf:Vec<u8>=vec![0u8;self.log_size as usize];
        let log_size_bytes=self.log_size.to_le_bytes();
        let lsn_bytes=self.lsn.to_le_bytes();
        let task:u16=match self.task_type{
            TaskType::Write=>0,
            TaskType::Update=>1,
            TaskType::Delete=>2,
        };
        let task_bytes=task.to_le_bytes();
        buf[0..2].copy_from_slice(&log_size_bytes);
        buf[2..10].copy_from_slice(&lsn_bytes);
        buf[10..12].copy_from_slice(&task_bytes);
        let page_id_bytes=self.page_id.to_le_bytes();
        let record_id_bytes=self.record_id.to_le_bytes();
        buf[12..20].copy_from_slice(&page_id_bytes);
        buf[20..24].copy_from_slice(&record_id_bytes);
        if matches!(self.task_type,TaskType::Delete){
            return Ok(buf);
        }
        let record:&Vec<u8>=match data{
            Some(v)=>v,
            None=>return Err(DbError::InsufficientParams),
        };
        buf[24..self.log_size as usize].copy_from_slice(record);
        Ok(buf)
    }

    fn deserialise(wal : &File,offset:u64)->Result<(Self,Option<Vec<u8>>),DbError>{
        let mut buf_size=[0u8;2];
        wal.read_at(&mut buf_size,offset).map_err(|_| DbError::FileError)?;
        let size:u16=u16::from_le_bytes(buf_size[0..2].try_into().unwrap());
        if size<24{
            return Err(DbError::CorruptedWAL);
        }
        let mut log_buffer:Vec<u8>=vec![0u8;size as usize];
        if wal.read_at(&mut log_buffer,offset).map_err(|_| DbError::FileError)? !=size as usize{
            return Err(DbError::CorruptedWAL);
        }
        let lsn=u64::from_le_bytes(log_buffer[2..10].try_into().unwrap());
        let task=u16::from_le_bytes(log_buffer[10..12].try_into().unwrap());
        let task_type=match task{
            0=>TaskType::Write,
            1=>TaskType::Update,
            2=>TaskType::Delete,
            _=>return Err(DbError::CorruptedWAL),
        };
        let page_id=u64::from_le_bytes(log_buffer[12..20].try_into().unwrap());
        let record_id=u32::from_le_bytes(log_buffer[20..24].try_into().unwrap());
        let log=Self{
            log_size:size,
            lsn,
            task_type,
            page_id,
            record_id,
        };
        if matches!(log.task_type,TaskType::Delete){
            return Ok((log,None));
        }
        let mut data:Vec<u8>=vec![0u8;log.log_size as usize-24];
        data.copy_from_slice(&log_buffer[24..log.log_size as usize]);
        Ok((log,Some(data)))
    }
}

impl WAL{
    pub fn deserialise()->Result<Self,DbError>{
        let wal=File::options()
            .read(true)
            .write(true)
            .create(true)
            .open("wal.log")
            .map_err(|_| DbError::FileError)?;
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

    pub fn add_log(&mut self,task_type:TaskType,page_id:u64,record_id:u32,data:Option<&Vec<u8>>)->Result<(),DbError>{
        if data==None && !matches!(task_type,TaskType::Delete){
            return Err(DbError::InsufficientParams);
        }
        let entry:Log=match task_type{
            TaskType::Delete=>{
                let lsn=self.last_lsn+1;
                let log_size=24;
                let entry=Log{
                    log_size,
                    lsn,
                    task_type,
                    page_id,
                    record_id,
                };
                entry
            },
            TaskType::Write=>{
                let lsn:u64=self.last_lsn+1;
                let data_size:u16=match data{
                    Some(record)=>record.len() as u16,
                    None=>return Err(DbError::InsufficientParams),
                };
                let log_size:u16=24+data_size;
                let entry=Log{
                    log_size,
                    lsn,
                    task_type,
                    page_id,
                    record_id,
                };
                entry
            },
            TaskType::Update=>{
                let lsn:u64=self.last_lsn+1;
                let data_size:u16=match data{
                    Some(record)=>record.len() as u16,
                    None=>return Err(DbError::InsufficientParams),
                };
                let log_size:u16=24+data_size;
                let entry=Log{
                    log_size,
                    lsn,
                    task_type,
                    page_id,
                    record_id,
                };
                entry
            },
        };
        let buf:Vec<u8>=entry.serialise(data)?;
        self.wal.write_all_at(&buf,self.file_size).map_err(|_| DbError::FileError)?;
        self.length+=1;
        self.last_lsn+=1;
        let len_buf=self.length.to_le_bytes();
        let lsn_buf=self.last_lsn.to_le_bytes();
        self.wal.write_all_at(&len_buf,8).map_err(|_| DbError::FileError)?;
        self.wal.write_all_at(&lsn_buf,0).map_err(|_| DbError::FileError)?;
        self.wal.sync_data().map_err(|_| DbError::FileError)?;
        self.file_size+=entry.log_size as u64;
        Ok(())
    }

    pub fn find_next_log(&self,last_lsn:u64,page_id:u64,iterator:Option<u64>)->Result<Option<(Log,Option<Vec<u8>>,u64)>,DbError>{
        let mut offset:u64=match iterator{
            Some(v)=>{
                if v<10{
                    return Err(DbError::FileError);
                }
                v
            },
            None=>10,
        };
        while offset<self.file_size{
            let (log,data)=Log::deserialise(&self.wal,offset)?;
            if log.page_id==page_id && log.lsn>last_lsn{
                let new_offset:u64=offset+log.log_size as u64;
                return Ok(Some((log,data,new_offset)));
            }
            offset+=log.log_size as u64;
        }
        Ok(None)
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

    pub fn reset(&mut self)->Result<(),DbError>{
        self.wal.set_len(10).map_err(|_| DbError::FileError)?;
        self.length=0;
        self.file_size=10;
        let len_buf=self.length.to_le_bytes();
        self.wal.write_all_at(&len_buf,8).map_err(|_| DbError::FileError)?;
        self.wal.sync_data().map_err(|_| DbError::FileError)?;
        Ok(())
    }
}
