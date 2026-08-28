//Code by Sohum Pathak
//sohum.pathak@protonmail.com

use crate::slotted_page::{Page,SLOT_SIZE};
use crate::page::{DatabaseFile,PAGE_SIZE};
use crate::db_errors::DbError;
use crate::lru_cache::{LRUCache,DLLNode};
use std::os::unix::prelude::FileExt;

pub struct BufferPool{
    pub lru : LRUCache,
    pub db_file : DatabaseFile,
}

impl BufferPool{
    pub fn new(capacity : usize,db_file : DatabaseFile)->Self{
        let lru=LRUCache::new(capacity);
        Self{
            lru,
            db_file,
        }
    }
    pub fn find_free_page(&self,size:usize)->Result<u64,DbError>{
        let mut curr=0;
        let mut search_needed=true;
        match self.lru.dll.head{
            Some(idx)=>curr=idx,
            None=>search_needed=false,
        }
        while search_needed{
            let node:&DLLNode=&self.lru.dll.nodes[curr];
            if node.page.has_space(size){
                return Ok(node.page.header.page_id);
            }
            match node.next{
                Some(idx)=>curr=idx,
                None=>search_needed=false,
            };
        }
        let mut offset:u64=0;
        let mut buf=[0u8;10];
        while offset+10<=(self.db_file.size/PAGE_SIZE as u64)*10{
            match self.db_file.page_metadata.read_at(&mut buf,offset){
                Ok(_)=>{},
                Err(_)=>return Err(DbError::FileError),
            };
            let page_id=u64::from_le_bytes(buf[0..8].try_into().unwrap());
            let size_x=u16::from_le_bytes(buf[8..10].try_into().unwrap());
            if size_x as usize>=size+SLOT_SIZE{
                return Ok(page_id);
            }
            offset+=10;
        }
        Err(DbError::SpaceOver)
    }
    pub fn get_page(&mut self, page_id: u64) -> Result<&Page, DbError> {
        let idx = match self.lru.get_index(page_id) {
            Ok(idx) => idx,
            Err(_) => {
                let page = Page::load(page_id, &self.db_file)?;
                self.lru.set_new(page, &self.db_file)?;
                match self.lru.dll.head {
                    Some(h) => h,
                    None => return Err(DbError::RecordAbsent),
                }
            }
        };
        Ok(&self.lru.dll.nodes[idx].page)
    }
    pub fn get_page_mut(&mut self, page_id: u64) -> Result<&mut Page, DbError> {
        let idx = match self.lru.get_index(page_id) {
            Ok(idx) => idx,
            Err(_) => {
                let page = Page::load(page_id, &self.db_file)?;
                self.lru.set_new(page, &self.db_file)?;
                match self.lru.dll.head {
                    Some(h) => h,
                    None => return Err(DbError::CorruptedLRU),
                }
            }
        };

        Ok(&mut self.lru.dll.nodes[idx].page)
    }
    pub fn evict_all(&mut self)->Result<(),DbError>{
        let n=self.lru.dll.nodes.len();
        while self.lru.dll.trash.len()!=n{
            let x=self.lru.dll.pop_tail(&self.db_file)?;
            //WAL
            let x=self.lru.dll.nodes[x].page.header.page_id;
            self.lru.map.remove(&x);
        }
        Ok(())
    }
    pub fn allocate_page(&mut self,lsn:u64)->Result<u64,DbError>{
        self.db_file.allocate_page(lsn)
    }
    pub fn evict_page(&mut self,page_id:u64)->Result<(),DbError>{
        let node=self.lru.get_mut(page_id)?;
        node.page.flush(&mut self.db_file)?;
        self.lru.delete(page_id)?;
        Ok(())
    }

    pub fn flush_page(&mut self,page_id:u64)->Result<(),DbError>{
        let node=self.lru.get_mut(page_id)?;
        node.page.flush(&mut self.db_file)?;
        Ok(())
    }

    pub fn flush_all(&mut self)->Result<(),DbError>{
        let mut curr=match self.lru.dll.head{
            Some(idx)=>idx,
            None=>return Ok(()),
        };
        loop{
            self.lru.dll.nodes[curr].page.flush(&self.db_file)?;
            match self.lru.dll.nodes[curr].next{
                Some(idx)=>{
                    curr=idx;
                },
                None=>break,
            };
        }
        Ok(())
    }
}

