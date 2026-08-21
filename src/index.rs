use crate::db_errors::DbError;
use crate::buffer_pool::BufferPool;
use crate::b_plus_tree::BPlusTree;
use crate::page::{DatabaseFile,PageType,PageHeader,PAGE_SIZE};
use crate::slotted_page::{RECORD_SIZE,Page};

pub const POOL_CAPACITY:usize=8; //magic number for now

pub struct Index{
    pub tree            :   BPlusTree,
    pub pool            :   BufferPool,
    pub next_record_id  :   u16,
}

impl Index{
    pub fn new(db_file:DatabaseFile)->Self{
        let tree=BPlusTree::new();
        let pool=BufferPool::new(POOL_CAPACITY,db_file);
        let next_record_id:u16=1;
        Self{
            tree,
            pool,
            next_record_id,
        }
    }
    pub fn get_record(&mut self,record_id : u16,record_buf : &mut [u8;RECORD_SIZE])->Result<usize,DbError>{
        let page_id:u64=self.tree.search(record_id)?;
        let page:&Page=self.pool.get_page(page_id)?;
        page.read_record(record_id,record_buf)
    }
    pub fn write_record(&mut self,buf:&[u8;RECORD_SIZE],size:usize)->Result<usize,DbError>{
        let page_id:u64=match self.pool.find_free_page(){   //currently only gives the cached free
            //page
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
            page.write_record(self.next_record_id,buf,size)?;
            page.free_space()
        };
        self.pool.db_file.edit_page_metadata(page_id,free_space as usize)?;
        self.tree.insert(self.next_record_id,page_id)?;
        self.next_record_id+=1;
        Ok(size)
    }
    pub fn update_record(&mut self,record_id:u16,buf:&[u8;RECORD_SIZE],size:usize)->Result<usize,DbError>{
        let page_id=self.tree.search(record_id)?;
        let page:&mut Page=self.pool.get_page_mut(page_id)?;
        page.update_record(record_id,buf,size)
    }
    pub fn delete_record(&mut self,record_id:u16)->Result<(),DbError>{
        let page_id=self.tree.search(record_id)?;
        let free_space={
            let page:&mut Page=self.pool.get_page_mut(page_id)?;
            page.delete_record(record_id)?;
            page.free_space()
        };
        self.pool.db_file.edit_page_metadata(page_id,free_space as usize)?;
        self.tree.delete(record_id)?;
        Ok(())
    }
}
