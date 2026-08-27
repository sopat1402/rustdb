//Code by Sohum Pathak
//sohum.pathak@protonmail.com

use crate::db_errors::DbError;
use crate::buffer_pool::BufferPool;
use crate::b_plus_tree::BPlusTree;
use crate::page::{DatabaseFile,PageType,PageHeader,PAGE_SIZE};
use crate::slotted_page::{Page};
use crate::wal::{WAL,TaskType};

const POOL_CAPACITY:usize=16; //magic number for now
const CHECKPOINT_MAX:usize=16*1024*1024;

pub struct Index{
    tree            :   BPlusTree,
    pool            :   BufferPool,
    pub wal             :   WAL,
    pub next_record_id  :   u32,
}

impl Index{
    pub fn new(mut db_file: DatabaseFile) -> Result<Self, DbError> {
        let tree = BPlusTree::deserialise(&mut db_file)?;
        let pool = BufferPool::new(POOL_CAPACITY, db_file);
        let wal = WAL::deserialise()?;
        let next_record_id: u32 = tree.max_key().map(|k| k + 1).unwrap_or(1);
        Ok(Self {
            tree,
            pool,
            wal,
            next_record_id,
        })
    }
    pub fn get_record(&mut self,record_id : u32)->Result<Vec<u8>,DbError>{
        let page_id:u64=self.tree.search(record_id)?;
        let page:&mut Page=self.pool.get_page_mut(page_id)?;
        let buf:Vec<u8>=match page.read_record(record_id){
            Ok(v)=>v,
            Err(DbError::CorruptedDataError)=>{
                self.reconstruct(page_id)?;
                self.get_record(record_id)?
            },
            Err(e)=>return Err(e),
        };
        Ok(buf)
    }
    pub fn write_record(&mut self,buf:&[u8],size:usize)->Result<u32,DbError>{
        let page_id:u64=match self.pool.find_free_page(size){
            Ok(id)=>id,
            Err(DbError::SpaceOver)=>{
                let id=self.pool.allocate_page(self.wal.last_lsn)?;
                let header=PageHeader::new(id,PageType::Data,self.wal.last_lsn);
                let buf=[0u8;PAGE_SIZE];
                let page=Page::new(header,buf);
                self.pool.lru.set_new(page,&self.pool.db_file)?;
                id
            },
            Err(e)=>return Err(e),
        };
        let free_space={
            let page:&mut Page=self.pool.get_page_mut(page_id)?;
            self.wal.add_log(TaskType::Write,page_id,self.next_record_id,Some(buf))?;
            match page.write_record(self.next_record_id,buf,size){
                Ok(_)=>page.header.lsn=self.wal.last_lsn,
                Err(DbError::CorruptedDataError)=>self.reconstruct(page_id)?,
                Err(e)=>return Err(e),
            };
            {
                let page=self.pool.get_page(page_id)?;
                page.free_space()
            }
        };
        self.pool.db_file.edit_page_metadata(page_id,free_space as usize)?;
        self.tree.insert(self.next_record_id,page_id)?;
        self.next_record_id+=1;
        self.checkpoint(false)?;
        Ok(self.next_record_id-1)
    }
    pub fn update_record(&mut self,record_id:u32,buf:&[u8],size:usize)->Result<(),DbError>{
        let page_id=self.tree.search(record_id)?;
        let page:&mut Page=self.pool.get_page_mut(page_id)?;
        self.wal.add_log(TaskType::Update,page_id,record_id,Some(buf))?;
        match page.update_record(record_id,buf,size){
            Ok(_)=>{
                page.header.lsn=self.wal.last_lsn;
                let free_space=page.free_space();
                self.pool.db_file.edit_page_metadata(page_id,free_space as usize)?;
            },
            Err(DbError::SpaceOver)=>{
                page.header.lsn=self.wal.last_lsn;
                let old_free_space=page.free_space();
                self.pool.db_file.edit_page_metadata(page_id,old_free_space as usize)?;
                let page_id:u64=match self.pool.find_free_page(size){
                    Ok(id)=>{
                        id
                    },
                    Err(DbError::SpaceOver)=>{
                        let id=self.pool.allocate_page(self.wal.last_lsn)?;
                        let header=PageHeader::new(id,PageType::Data,self.wal.last_lsn);
                        let buf=[0u8;PAGE_SIZE];
                        let page=Page::new(header,buf);
                        self.pool.lru.set_new(page,&self.pool.db_file)?;
                        id
                    },
                    Err(e)=>return Err(e),
                };
                let free_space={
                    let page:&mut Page=self.pool.get_page_mut(page_id)?;
                    self.wal.add_log(TaskType::Write,page_id,record_id,Some(buf))?;
                    match page.write_record(record_id,buf,size){
                        Ok(_)=>page.header.lsn=self.wal.last_lsn,
                        Err(DbError::CorruptedDataError)=>self.reconstruct(page_id)?,
                        Err(e)=>{
                            return Err(e)
                        },
                    };
                    {
                        let page=self.pool.get_page(page_id)?;
                        page.free_space()
                    }
                };
                self.pool.db_file.edit_page_metadata(page_id,free_space as usize)?;
                self.tree.delete(record_id)?;
                self.tree.insert(record_id,page_id)?;
            },
            Err(DbError::CorruptedDataError)=>{
                self.reconstruct(page_id)?;
                let page=self.pool.get_page_mut(page_id)?;
                match page.read_record(record_id){
                    Ok(_)=>{},
                    Err(DbError::RecordAbsent)=>{
                        let old_free_space=page.free_space();
                        self.pool.db_file.edit_page_metadata(page_id,old_free_space as usize)?;
                        let page_id:u64=match self.pool.find_free_page(size){
                            Ok(id)=>id,
                            Err(DbError::SpaceOver)=>{
                                let id=self.pool.allocate_page(self.wal.last_lsn)?;
                                let header=PageHeader::new(id,PageType::Data,self.wal.last_lsn);
                                let buf=[0u8;PAGE_SIZE];
                                let page=Page::new(header,buf);
                                self.pool.lru.set_new(page,&self.pool.db_file)?;
                                id
                            },
                            Err(e)=>return Err(e),
                        };
                        let free_space={
                            let page:&mut Page=self.pool.get_page_mut(page_id)?;
                            self.wal.add_log(TaskType::Write,page_id,record_id,Some(buf))?;
                            match page.write_record(record_id,buf,size){
                                Ok(_)=>page.header.lsn=self.wal.last_lsn,
                                Err(DbError::CorruptedDataError)=>self.reconstruct(page_id)?,
                                Err(e)=>return Err(e),
                            };
                            {
                                let page=self.pool.get_page(page_id)?;
                                page.free_space()
                            }
                        };
                        self.pool.db_file.edit_page_metadata(page_id,free_space as usize)?;
                        self.tree.delete(record_id)?;
                        self.tree.insert(record_id,page_id)?;
                    },
                    Err(e)=>return Err(e),
                };
            },
            Err(e)=>return Err(e),
        }
        self.checkpoint(false)?;
        Ok(())
    }

    pub fn update_record_recover(&mut self,record_id:u32,buf:&[u8],size:usize,lsn:u64)->Result<(),DbError>{
        let page_id=self.tree.search(record_id)?;
        let page:&mut Page=self.pool.get_page_mut(page_id)?;
        match page.update_record(record_id,buf,size){
            Ok(_)=>{
                page.header.lsn=lsn;
                let free_space=page.free_space();
                self.pool.db_file.edit_page_metadata(page_id,free_space as usize)?;
            },
            Err(DbError::SpaceOver)=>{
                page.header.lsn=lsn;
                let old_free_space=page.free_space();
                self.pool.db_file.edit_page_metadata(page_id,old_free_space as usize)?;
                let page_id:u64=match self.pool.find_free_page(size){
                    Ok(id)=>{
                        id
                    },
                    Err(DbError::SpaceOver)=>{
                        let id=self.pool.allocate_page(self.wal.last_lsn)?;
                        let header=PageHeader::new(id,PageType::Data,self.wal.last_lsn);
                        let buf=[0u8;PAGE_SIZE];
                        let page=Page::new(header,buf);
                        self.pool.lru.set_new(page,&self.pool.db_file)?;
                        id
                    },
                    Err(e)=>return Err(e),
                };
                let free_space={
                    let page:&mut Page=self.pool.get_page_mut(page_id)?;
                    self.wal.add_log(TaskType::Write,page_id,record_id,Some(buf))?;
                    match page.write_record(record_id,buf,size){
                        Ok(_)=>page.header.lsn=self.wal.last_lsn,
                        Err(DbError::CorruptedDataError)=>self.reconstruct(page_id)?,
                        Err(e)=>{
                            return Err(e)
                        },
                    };
                    {
                        let page=self.pool.get_page(page_id)?;
                        page.free_space()
                    }
                };
                self.pool.db_file.edit_page_metadata(page_id,free_space as usize)?;
                self.tree.delete(record_id)?;
                self.tree.insert(record_id,page_id)?;
            },
            Err(DbError::CorruptedDataError)=>{
                self.reconstruct(page_id)?;
                let page=self.pool.get_page_mut(page_id)?;
                match page.read_record(record_id){
                    Ok(_)=>{},
                    Err(DbError::RecordAbsent)=>{
                        let old_free_space=page.free_space();
                        self.pool.db_file.edit_page_metadata(page_id,old_free_space as usize)?;
                        let page_id:u64=match self.pool.find_free_page(size){
                            Ok(id)=>id,
                            Err(DbError::SpaceOver)=>{
                                let id=self.pool.allocate_page(self.wal.last_lsn)?;
                                let header=PageHeader::new(id,PageType::Data,self.wal.last_lsn);
                                let buf=[0u8;PAGE_SIZE];
                                let page=Page::new(header,buf);
                                self.pool.lru.set_new(page,&self.pool.db_file)?;
                                id
                            },
                            Err(e)=>return Err(e),
                        };
                        let free_space={
                            let page:&mut Page=self.pool.get_page_mut(page_id)?;
                            self.wal.add_log(TaskType::Write,page_id,record_id,Some(buf))?;
                            match page.write_record(record_id,buf,size){
                                Ok(_)=>page.header.lsn=self.wal.last_lsn,
                                Err(DbError::CorruptedDataError)=>self.reconstruct(page_id)?,
                                Err(e)=>return Err(e),
                            };
                            {
                                let page=self.pool.get_page(page_id)?;
                                page.free_space()
                            }
                        };
                        self.pool.db_file.edit_page_metadata(page_id,free_space as usize)?;
                        self.tree.delete(record_id)?;
                        self.tree.insert(record_id,page_id)?;
                    },
                    Err(e)=>return Err(e),
                };
            },
            Err(e)=>return Err(e),
        }
        Ok(())
    }
    pub fn delete_record(&mut self,record_id:u32)->Result<(),DbError>{
        let page_id=self.tree.search(record_id)?;
        let free_space={
            let page:&mut Page=self.pool.get_page_mut(page_id)?;
            self.wal.add_log(TaskType::Delete,page_id,record_id,None)?;
            match page.delete_record(record_id){
                Ok(_)=>page.header.lsn=self.wal.last_lsn,
                Err(DbError::CorruptedDataError)=>{
                    self.reconstruct(page_id)?;
                },
                Err(e)=>return Err(e),
            };
            {
                let page=self.pool.get_page(page_id)?;
                page.free_space()
            }
        };
        self.pool.db_file.edit_page_metadata(page_id,free_space as usize)?;
        self.tree.delete(record_id)?;
        self.checkpoint(false)?;
        Ok(())
    }
    pub fn shutdown(&mut self)->Result<(),DbError>{
        self.checkpoint(true)?;
        self.pool.evict_all()?;
        self.tree.serialise(&mut self.pool.db_file)?;
        self.wal.reset()?;
        Ok(())
    }

    pub fn recover(&mut self)->Result<(),DbError>{
        let mut entry=self.wal.get_log_any(None)?;
        while let Some((log,data,iterator)) = entry{
            {
                let page:&Page=self.pool.get_page(log.page_id)?;
                if page.header.lsn>=log.lsn{
                    entry=self.wal.get_log_any(Some(iterator))?;
                    continue;
                }
            }
            match log.task_type{
                TaskType::Delete=>{
                    let page:&mut Page=self.pool.get_page_mut(log.page_id)?;
                    match page.delete_record(log.record_id){
                        Ok(_)=>{
                            self.tree.delete(log.record_id)?;
                            page.header.lsn=log.lsn;
                        },
                        Err(DbError::CorruptedDataError)=>self.reconstruct(log.page_id)?,
                        Err(e)=>return Err(e),
                    };
                },
                TaskType::Write=>{
                    let page:&mut Page=self.pool.get_page_mut(log.page_id)?;
                    let buf=match data{
                        Some(rec)=>rec,
                        None=>return Err(DbError::CorruptedWAL),
                    };
                    match page.write_record(log.record_id,&buf,buf.len()){
                        Ok(_)=>{
                            self.tree.insert(log.record_id,log.page_id)?;
                            self.next_record_id+=1;
                            page.header.lsn=log.lsn;
                        },
                        Err(DbError::SpaceOver)=>{
                            page.header.lsn=log.lsn;
                            let new_page_id=self.pool.find_free_page(buf.len())?;
                            let new_page=self.pool.get_page_mut(new_page_id)?;
                            match new_page.write_record(log.record_id,&buf,buf.len()){
                                Ok(_)=>{
                                    if new_page.header.lsn<log.lsn{
                                        new_page.header.lsn=log.lsn;
                                    }
                                },
                                Err(DbError::CorruptedDataError)=>{
                                    new_page.write_record(log.record_id,&buf,buf.len())?;
                                    self.tree.insert(log.record_id,new_page_id)?;
                                    self.next_record_id+=1;
                                },
                                Err(e)=>return Err(e),
                            };
                        },
                        Err(DbError::CorruptedDataError)=>{
                            self.reconstruct(log.page_id)?;
                        },
                        Err(e)=>return Err(e),
                    };
                },
                TaskType::Update=>{
                    let buf=match data{
                        Some(rec)=>rec,
                        None=>return Err(DbError::CorruptedWAL),
                    };
                    self.update_record_recover(log.record_id,&buf,buf.len(),log.lsn)?;
                },
            };
            entry=self.wal.get_log_any(Some(iterator))?;
        }
        self.pool.evict_all()?;
        Ok(())
    }

    fn checkpoint(&mut self,force:bool)->Result<(),DbError>{
        if force || (self.wal.file_size >= CHECKPOINT_MAX as u64){
            let mut entry=self.wal.get_log_any(None)?;
            while let Some((log,data,iterator))=entry{
                {
                    let page:&mut Page=self.pool.get_page_mut(log.page_id)?;
                    if page.header.lsn>=log.lsn{
                        entry=self.wal.get_log_any(Some(iterator))?;
                        continue;
                    }
                }
                match log.task_type{
                    TaskType::Delete=>{
                        let page=self.pool.get_page_mut(log.page_id)?;
                        match page.delete_record(log.record_id){
                            Ok(_)=>page.header.lsn=log.lsn,
                            Err(DbError::CorruptedDataError)=>self.reconstruct(log.page_id)?,
                            Err(e)=>return Err(e),
                        };
                    },
                    TaskType::Write=>{
                        let page=self.pool.get_page_mut(log.page_id)?;
                        let buf=match data{
                            Some(val)=>val,
                            None=>return Err(DbError::CorruptedWAL),
                        };
                        match page.write_record(log.record_id,&buf,log.log_size as usize-24){
                            Ok(_)=>page.header.lsn=log.lsn,
                            Err(DbError::CorruptedDataError)=>self.reconstruct(log.page_id)?,
                            Err(DbError::SpaceOver)=>{},
                            Err(e)=>return Err(e),
                        };
                    },
                    TaskType::Update=>{
                        let page=self.pool.get_page_mut(log.page_id)?;
                        let buf=match data{
                            Some(val)=>val,
                            None=>return Err(DbError::CorruptedWAL),
                        };
                        match page.update_record(log.record_id,&buf,log.log_size as usize-24){
                            Ok(_)=>page.header.lsn=log.lsn,
                            Err(DbError::CorruptedDataError)=>self.reconstruct(log.page_id)?,
                            Err(DbError::SpaceOver)=>{},
                            Err(e)=>return Err(e),
                        };
                    },
                };
                {
                    let page=self.pool.get_page_mut(log.page_id)?;
                    page.header.lsn=log.lsn;
                }
                entry=self.wal.get_log_any(Some(iterator))?;
            }
            self.pool.flush_all()?;
            self.wal.reset()?;
        }
        Ok(())
    }

    fn reconstruct(&mut self,page_id:u64)->Result<(),DbError>{
        self.pool.lru.delete(page_id)?;
        let page:&mut Page=self.pool.get_page_mut(page_id)?;
        let mut last_lsn=page.header.lsn;
        let mut entry=self.wal.find_next_log(last_lsn,page_id,None)?;
        while let Some((log,data,iterator))=entry{
            match log.task_type{
                TaskType::Delete=>{
                    page.delete_record(log.record_id)?;
                },
                TaskType::Write=>{
                    if let Some(buf)=data{
                        match page.write_record(log.record_id,&buf,log.log_size as usize-24){
                            Ok(_)=>{},
                            Err(DbError::SpaceOver)=>{},
                            Err(e)=>return Err(e),
                        };
                    }
                    else{
                        return Err(DbError::CorruptedWAL);
                    }
                },
                TaskType::Update=>{
                    if let Some(buf)=data{
                        match page.update_record(log.record_id,&buf,log.log_size as usize-24){
                            Ok(_)=>{},
                            Err(DbError::SpaceOver)=>{},
                            Err(e)=>return Err(e),
                        };
                    }
                    else{
                        return Err(DbError::CorruptedWAL);
                    }
                },
            };
            last_lsn=log.lsn;
            entry=self.wal.find_next_log(last_lsn,page_id,Some(iterator))?;
        }
        match self.pool.get_page_mut(page_id){
            Ok(page)=>page.header.lsn=last_lsn,
            Err(e)=>return Err(e),
        };
        self.pool.flush_page(page_id)?;
        Ok(())
    }
}
