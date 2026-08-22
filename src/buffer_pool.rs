//Code by Sohum Pathak
//sohum.pathak@protonmail.com
use crate::slotted_page::{Page,RECORD_SIZE,SLOT_SIZE};
use crate::page::{DatabaseFile,PAGE_HEADER_SIZE,PAGE_SIZE};
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
    pub fn find_free_page(&self)->Result<u64,DbError>{
        let mut curr=0;
        let mut search_needed=true;
        match self.lru.dll.head{
            Some(idx)=>curr=idx,
            None=>search_needed=false,
        }
        while search_needed{
            let node:&DLLNode=&self.lru.dll.nodes[curr];
            if node.page.has_space(){
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
            let size=u16::from_le_bytes(buf[8..10].try_into().unwrap());
            if PAGE_SIZE-PAGE_HEADER_SIZE-size as usize>=RECORD_SIZE+SLOT_SIZE{
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
                let page = match Page::load(page_id as u16, &self.db_file) {
                    Ok(p) => p,
                    Err(e) => return Err(e),
                };

                match self.lru.set_new(page, &self.db_file) {
                    Ok(_) => {}
                    Err(e) => return Err(e),
                };

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
                let page = match Page::load(page_id as u16, &self.db_file) {
                    Ok(p) => p,
                    Err(e) => return Err(e),
                };

                match self.lru.set_new(page, &self.db_file) {
                    Ok(_) => {}
                    Err(e) => return Err(e),
                };

                match self.lru.dll.head {
                    Some(h) => h,
                    None => return Err(DbError::RecordAbsent),
                }
            }
        };

        Ok(&mut self.lru.dll.nodes[idx].page)
    }
    pub fn evict_all(&mut self)->Result<(),DbError>{
        let n=self.lru.dll.nodes.len();
        while self.lru.dll.trash.len()!=n{
            let x=match self.lru.dll.pop_tail(&self.db_file){ //flush is done inside here
                Ok(val)=>val,
                Err(e)=>return Err(e),
            };
            let x=self.lru.dll.nodes[x].page.header.page_id;
            self.lru.map.remove(&x);
        }
        Ok(())
    }
    pub fn allocate_page(&mut self)->Result<u64,DbError>{
        let x=match self.db_file.allocate_page(){
            Ok(id)=>id,
            Err(e)=>return Err(e),
        };
        Ok(x)
    }
    pub fn evict_page(&mut self,page_id:u64)->Result<(),DbError>{
        let node=match self.lru.get_mut(page_id){
            Ok(v)=>v,
            Err(e)=>return Err(e),
        };
        match node.page.flush(&mut self.db_file){
            Ok(_)=>{},
            Err(e)=>return Err(e),
        };
        match self.lru.delete(page_id){
            Ok(_)=>{},
            Err(e)=>return Err(e),
        };
        Ok(())
    }

    pub fn flush_page(&mut self,page_id:u64)->Result<(),DbError>{
        let node=match self.lru.get_mut(page_id){
            Ok(v)=>v,
            Err(e)=>return Err(e),
        };
        match node.page.flush(&mut self.db_file){
            Ok(_)=>{},
            Err(e)=>return Err(e),
        };
        Ok(())
    }

    pub fn flush_all(&mut self)->Result<(),DbError>{
        let mut curr=match self.lru.dll.head{
            Some(idx)=>idx,
            None=>return Ok(()), //lru was empty
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

