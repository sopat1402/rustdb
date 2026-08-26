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
    wal             :   WAL,
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
            Err(e)=>return Err(e),
        };
        Ok(buf)
    }
    pub fn write_record(&mut self,buf:&[u8],size:usize)->Result<usize,DbError>{
        let page_id:u64=match self.pool.find_free_page(size){
            Ok(id)=>id,
            Err(DbError::SpaceOver)=>{
                let id=self.pool.allocate_page()?;
                let header=PageHeader::new(id,PageType::Data);
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
            page.write_record(self.next_record_id,buf,size)?;
            page.free_space()
        };
        self.pool.db_file.edit_page_metadata(page_id,free_space as usize)?;
            //page
        self.tree.insert(self.next_record_id,page_id)?;
        self.next_record_id+=1;
        Ok(size)
    }
    pub fn update_record(&mut self,record_id:u32,buf:&[u8],size:usize)->Result<usize,DbError>{
        let page_id=self.tree.search(record_id)?;
        let page:&mut Page=self.pool.get_page_mut(page_id)?;
        self.wal.add_log(TaskType::Update,page_id,record_id,Some(buf))?;
        match page.update_record(record_id,buf,size){
            Ok(_)=>{
                let free_space=page.free_space();
                self.pool.db_file.edit_page_metadata(page_id,free_space as usize)?;
            },
            Err(DbError::SpaceOver)=>{
                let old_free_space=page.free_space();
                self.pool.db_file.edit_page_metadata(page_id,old_free_space as usize)?;
                let page_id:u64=match self.pool.find_free_page(size){
                    Ok(id)=>id,
                    Err(DbError::SpaceOver)=>{
                        let id=self.pool.allocate_page()?;
                        let header=PageHeader::new(id,PageType::Data);
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
                    page.write_record(record_id,buf,size)?;
                    page.free_space()
                };
                self.pool.db_file.edit_page_metadata(page_id,free_space as usize)?;
                self.tree.delete(record_id)?;
                self.tree.insert(record_id,page_id)?;
            },
            Err(e)=>return Err(e), //check for corrupted data error later
        }
        Ok(size)
    }
    pub fn delete_record(&mut self,record_id:u32)->Result<(),DbError>{
        let page_id=self.tree.search(record_id)?;
        let free_space={
            let page:&mut Page=self.pool.get_page_mut(page_id)?;
            self.wal.add_log(TaskType::Delete,page_id,record_id,None)?;
            page.delete_record(record_id)?;
            page.free_space()
        };
        self.pool.db_file.edit_page_metadata(page_id,free_space as usize)?;
        self.tree.delete(record_id)?;
        Ok(())
    }
    pub fn shutdown(&mut self)->Result<(),DbError>{
        //checkpoint with force flag
        self.pool.evict_all()?;
        self.tree.serialise(&mut self.pool.db_file)?;
        self.wal.reset()?;
        Ok(())
    }
}
