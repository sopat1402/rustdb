use crate::page::PageHeader;
use crate::page::PAGE_SIZE;
use std::vec::Vec;
use crate::page::DatabaseFile;
use crate::db_errors::DbError;
use crate::page::PAGE_HEADER_SIZE;

pub const RECORD_SIZE : usize =128;

pub struct Slot{
    pub id:u16,
    pub offset:u16,
    pub size:u16,
}

impl Slot{
    pub fn deserialise(buffer:&[u8;PAGE_SIZE],offset : usize)->Self{
        let id=u16::from_le_bytes(buffer[offset..offset+2].try_into().unwrap());
        let off=u16::from_le_bytes(buffer[offset+2..offset+4].try_into().unwrap());
        let size=u16::from_le_bytes(buffer[offset+4..offset+6].try_into().unwrap());
        Self{
            id,
            offset:off,
            size,
        }
    }
}

pub struct Page{
    pub header  :   PageHeader,
    pub buffer  :   [u8;PAGE_SIZE],
    pub trash   :   Vec<u16>,
}

impl Page{
    pub fn has_space(&self)->bool{
        (self.header.upper-self.header.lower)>=(RECORD_SIZE as u16+6)
    }
    pub fn load(id:u16,db_file : &DatabaseFile)->Result<Self,DbError>{
        let mut buf=[0u8;PAGE_SIZE];
        match db_file.read_page(id as u64,&mut buf){
            Ok(_)=>{},
            Err(e)=>return Err(e),
        };
        let header:PageHeader=match PageHeader::deserialise(&mut buf){
            Ok(head)=>head,
            Err(_)=>return Err(DbError::CorruptedDataError),
        };
        let mut offset:u64=PAGE_HEADER_SIZE as u64;
        let mut trash:Vec<u16>=Vec::new();
        while offset<header.lower as u64{
            let size=u16::from_le_bytes(buf[offset as usize+4..offset as usize+6].try_into().unwrap());
            if size==0{
                trash.push(offset as u16);
            }
            offset+=6;
        }
        Ok(Self{
            header,
            buffer:buf,
            trash:trash,
        })
    }
    pub fn flush(&mut self,db_file : &DatabaseFile)->Result<(),std::io::Error>{
        self.header.serialise(&mut self.buffer);
        db_file.write_page(self.header.page_id,&self.buffer)?;
        Ok(())
    }
    pub fn new(head : PageHeader,buf : [u8;PAGE_SIZE])->Self{
        let trash:Vec<u16>=Vec::new();
        Self{
            header : head,
            buffer : buf,
            trash,
        }
    }
    pub fn read_record(&self,record_id : u16,buf:&mut [u8;RECORD_SIZE])->Result<usize,DbError>{
        let mut offset:usize=PAGE_HEADER_SIZE;
        let mut slot : Option<Slot>=None;
        for _ in 0..self.header.item_count{
            let candidate=Slot::deserialise(&self.buffer,offset);
            if candidate.id==record_id {
                slot=Some(candidate);
                break;
            }
            offset+=6;
        }
        let slot=match slot{
            Some(s)=>s,
            None=>return Err(DbError::RecordAbsent),
        };
        offset=slot.offset as usize;
        let bytes_read=slot.size as usize;
        if bytes_read>RECORD_SIZE{
            return Err(DbError::RecordMismatch);
        }
        if slot.offset as usize>=PAGE_SIZE{
            return Err(DbError::RecordAbsent);
        }
        if slot.offset<self.header.upper{
            return Err(DbError::RecordAbsent);
        }
        if slot.size==0{
            return Err(DbError::RecordAbsent);
        }
        if offset + bytes_read > PAGE_SIZE {
            return Err(DbError::RecordMismatch);
        }
        buf[0..bytes_read].copy_from_slice(&self.buffer[offset..offset+bytes_read]);
        Ok(bytes_read)
    }

    pub fn write_record(&mut self,record_id:u16,buf:&[u8;RECORD_SIZE],size : usize)->Result<usize,DbError>{
        if size>RECORD_SIZE{
            return Err(DbError::SpaceOver);
        }
        if !self.trash.is_empty(){
            let slot:u16=match self.trash.pop(){
                Some(o)=>o,
                None=>return Err(DbError::CorruptedDataError),
            };
            let id_bytes=record_id.to_le_bytes();
            self.buffer[slot as usize..slot as usize+2].copy_from_slice(&id_bytes);
            let size_bytes=(size as u16).to_le_bytes();
            self.buffer[slot as usize+4..slot as usize+6].copy_from_slice(&size_bytes);
            let offset:u16=u16::from_le_bytes(self.buffer[slot as usize+2..slot as usize+4].try_into().unwrap());
            self.buffer[offset as usize..offset as usize+RECORD_SIZE].copy_from_slice(buf);
            return Ok(size)
        }
        let free_space=self.header.upper-self.header.lower;
        if free_space<6+size as u16{
            return Err(DbError::SpaceOver);
        }
        self.buffer[self.header.upper as usize-RECORD_SIZE..self.header.upper as usize].copy_from_slice(&buf[0..RECORD_SIZE]);
        self.header.upper-=RECORD_SIZE as u16;
        let mut slot_bytes=[0u8;6];
        let offset:usize=self.header.upper as usize;
        let offset_bytes=(offset as u16).to_le_bytes();
        let record_id_bytes=record_id.to_le_bytes();
        let size_bytes=(size as u16).to_le_bytes();  //will stay as size so the user doesn't get nulls
        slot_bytes[0..2].copy_from_slice(&record_id_bytes);
        slot_bytes[2..4].copy_from_slice(&offset_bytes);
        slot_bytes[4..6].copy_from_slice(&size_bytes);
        self.buffer[self.header.lower as usize..self.header.lower as usize+6].copy_from_slice(&slot_bytes);
        self.header.item_count+=1;
        self.header.lower+=6;
        Ok(size)
    }

    pub fn update_record(&mut self,record_id:u16,buf:&[u8;RECORD_SIZE],size:usize)->Result<usize,DbError>{
        if size>RECORD_SIZE{
            return Err(DbError::SpaceOver);
        }
        let mut offset:usize=PAGE_HEADER_SIZE;
        let mut slot : Option<Slot>=None;
        for _ in 0..self.header.item_count{
            let candidate=Slot::deserialise(&self.buffer,offset);
            if candidate.id==record_id {
                slot=Some(candidate);
                break;
            }
            offset+=6;
        }
        let slot=match slot{
            Some(s)=>s,
            None=>return Err(DbError::RecordAbsent),
        };
        let test_id=u16::from_le_bytes(self.buffer[slot.offset as usize..slot.offset as usize+2].try_into().unwrap()); //check if id changed
        if test_id!=slot.id{
            return Err(DbError::RecordMismatch);
        }
        if slot.size==0{
            return Err(DbError::RecordAbsent);
        }
        //will use offset to update the slot
        self.buffer[slot.offset as usize..slot.offset as usize+RECORD_SIZE].copy_from_slice(&buf[0..RECORD_SIZE]);
        let size_bites=(size as u16).to_le_bytes();
        self.buffer[offset+4..offset+6].copy_from_slice(&size_bites);
        Ok(size)
    }

    pub fn delete_record(&mut self,record_id:u16)->Result<(),DbError>{
        let mut offset:usize=PAGE_HEADER_SIZE;
        let mut slot : Option<Slot>=None;
        for _ in 0..self.header.item_count{
            let candidate=Slot::deserialise(&self.buffer,offset);
            if candidate.id==record_id {
                slot=Some(candidate);
                break;
            }
            offset+=6;
        }
        let slot=match slot{
            Some(s)=>s,
            None=>return Err(DbError::RecordAbsent),
        };
        self.trash.push(offset as u16);
        let bytes_read=slot.size as usize;
        if bytes_read>RECORD_SIZE{
            return Err(DbError::CorruptedDataError);
        }
        if slot.offset as usize>=PAGE_SIZE{
            return Err(DbError::CorruptedDataError);
        }
        if slot.offset<self.header.upper{
            return Err(DbError::CorruptedDataError);
        }
        let new_size:u16=0;
        let zero_bytes=new_size.to_le_bytes();
        self.buffer[offset+4..offset+6].copy_from_slice(&zero_bytes);
        Ok(())
    }
}

