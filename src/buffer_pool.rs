use crate::slotted_page::{Page};
use crate::page::{DatabaseFile};
use crate::db_errors::DbError;
use crate::lru_cache::{LRUCache,DLLNode};

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
        for i in 0..self.lru.dll.nodes.len(){
            let node:&DLLNode=&self.lru.dll.nodes[i];
            if node.page.has_space(){
                return Ok(node.page.header.page_id);
            }
        }
        Err(DbError::SpaceOver)
    }
    pub fn get_page(&mut self, page_id: u64) -> Result<&Page, DbError> {
        let idx = match self.lru.get_index(page_id) {
            Ok(idx) => idx,
            Err(_) => {
                let page = match Page::load(page_id as u16, &self.db_file) {
                    Ok(p) => p,
                    Err(_) => return Err(DbError::RecordAbsent),
                };

                match self.lru.set_new(page, &self.db_file) {
                    Ok(_) => {}
                    Err(_) => return Err(DbError::RecordMismatch),
                };

                match self.lru.dll.head {
                    Some(h) => h,
                    None => return Err(DbError::RecordMismatch),
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
                    Err(_) => return Err(DbError::RecordAbsent),
                };

                match self.lru.set_new(page, &self.db_file) {
                    Ok(_) => {}
                    Err(_) => return Err(DbError::RecordMismatch),
                };

                match self.lru.dll.head {
                    Some(h) => h,
                    None => return Err(DbError::RecordMismatch),
                }
            }
        };

        Ok(&mut self.lru.dll.nodes[idx].page)
    }
    pub fn flush_all(&mut self)->Result<(),DbError>{
        let n=self.lru.dll.nodes.len();
        while self.lru.dll.trash.len()!=n{
            let x=match self.lru.dll.pop_tail(&self.db_file){
                Ok(val)=>val,
                Err(_)=>return Err(DbError::CorruptedDataError),
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
    pub fn flush_page(&mut self,page_id:u64)->Result<(),DbError>{
        let node=match self.lru.get_mut(page_id){
            Ok(v)=>v,
            Err(_)=>return Err(DbError::PageAbsent),
        };
        match node.page.flush(&mut self.db_file){
            Ok(_)=>{},
            Err(_)=>return Err(DbError::RecordMismatch),
        };
        match self.lru.delete(page_id){
            Ok(_)=>{},
            Err(_)=>return Err(DbError::RecordMismatch),
        };
        Ok(())
    }
}

