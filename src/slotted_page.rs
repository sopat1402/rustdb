//Code by Sohum Pathak
//sohum.pathak@protonmail.com

use crate::page::{PageFlags,PageHeader,DatabaseFile,PAGE_HEADER_SIZE,PAGE_SIZE};
use std::vec::Vec;
use crate::db_errors::DbError;
use crate::crc32::crc32;

//pub const RECORD_SIZE : usize =256;
pub const SLOT_SIZE : usize=10;

pub struct Slot{
    pub id:u32,
    pub offset:u16,
    pub size:u32,
}

impl Slot{
    pub fn deserialise(buffer:&[u8;PAGE_SIZE],offset : usize)->Self{
        let id=u32::from_le_bytes(buffer[offset..offset+4].try_into().unwrap());
        let off=u16::from_le_bytes(buffer[offset+4..offset+6].try_into().unwrap());
        let size=u32::from_le_bytes(buffer[offset+6..offset+10].try_into().unwrap());
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
    pub trash   :   Vec<(u16,u32)>, //offset,size
}

impl Page{
    pub fn has_space(&self,size:usize)->bool{
        if matches!(self.header.flags,PageFlags::Corrupted){
            return false;
        }
        let max_trash_size=self.trash.iter().map(|(_, size)| *size).max().unwrap_or(0);
        let has_record_space:bool=((self.header.upper-self.header.lower)>=(SLOT_SIZE+size) as u16) || (max_trash_size>=size as u32);
        has_record_space
    }

    pub fn free_space(&self)->u32{
        if matches!(self.header.flags,PageFlags::Corrupted){
            return 0;
        }

        let total_trash_space:u32=self.trash.iter().map(|(_, size)| *size).sum();
        (self.header.upper-self.header.lower) as u32 + total_trash_space
    }

    fn check_checksum(buf : &mut [u8;PAGE_SIZE])->bool{
        //checksum is bytes 24 to 28
        let file_checksum=u32::from_le_bytes(buf[24..28].try_into().unwrap());
        let x:u32=0;
        let x_bytes:[u8;4]=x.to_le_bytes();
        buf[24..28].copy_from_slice(&x_bytes);
        let calc_checksum:u32=crc32(buf);
        let f_chk_bytes=file_checksum.to_le_bytes();
        buf[24..28].copy_from_slice(&f_chk_bytes);
        file_checksum==calc_checksum
    }

    pub fn load(id:u64,db_file : &DatabaseFile)->Result<Self,DbError>{
        let mut buf=[0u8;PAGE_SIZE];
        db_file.read_page(id,&mut buf)?;
        let header:PageHeader=PageHeader::deserialise(&mut buf)?;
        if matches!(header.flags,PageFlags::Corrupted){
            return Err(DbError::PageCorrupted);
        }
        let checksum_correct:bool=Page::check_checksum(&mut buf);
        if !checksum_correct{
            return Err(DbError::ChecksumMismatch);
        }
        let mut offset:u64=PAGE_HEADER_SIZE as u64;
        let mut trash:Vec<(u16,u32)>=Vec::new();
        while offset<header.lower as u64{
            let id=u32::from_le_bytes(buf[offset as usize+0..offset as usize+4].try_into().unwrap());
            let size=u32::from_le_bytes(buf[offset as usize+6..offset as usize+10].try_into().unwrap());
            if id==0{
                trash.push((offset as u16,size));
            }
            offset+=SLOT_SIZE as u64;
        }
        Ok(Self{
            header,
            buffer:buf,
            trash:trash,
        })
    }

    pub fn flush(&mut self,db_file : &DatabaseFile)->Result<(),DbError>{
        if matches!(self.header.flags,PageFlags::Corrupted){
            return Err(DbError::PageCorrupted);
        }
        if matches!(self.header.flags,PageFlags::Clean){
            return Ok(());
        }
        self.header.flags=PageFlags::Clean;
        self.header.checksum=0;
        self.header.serialise(&mut self.buffer);
        let chksum=crc32(&mut self.buffer);
        self.header.checksum=chksum;
        self.header.serialise(&mut self.buffer);
        match db_file.write_page(self.header.page_id,&self.buffer){
            Ok(_)=>{},
            Err(e)=>{
                self.header.flags=PageFlags::Dirty;
                return Err(e);
            }
        };
        Ok(())
    }

    pub fn new(head : PageHeader,buf : [u8;PAGE_SIZE])->Self{
        let trash:Vec<(u16,u32)>=Vec::new();
        let chks:u32=crc32(&buf);
        let mut page=Self{
            header : head,
            buffer : buf,
            trash,
        };
        page.header.checksum=chks;
        let chks_bytes=chks.to_le_bytes();
        page.buffer[24..28].copy_from_slice(&chks_bytes);
        page
    }

    pub fn read_record(&mut self,record_id : u32)->Result<Vec<u8>,DbError>{
        if matches!(self.header.flags,PageFlags::Corrupted){
            return Err(DbError::PageCorrupted);
        }
        let mut offset:usize=PAGE_HEADER_SIZE;
        let mut slot : Option<Slot>=None;
        for _ in 0..self.header.item_count{
            let candidate=Slot::deserialise(&self.buffer,offset);
            if candidate.id==record_id {
                slot=Some(candidate);
                break;
            }
            offset+=SLOT_SIZE;
        }
        let slot=match slot{
            Some(s)=>s,
            None=>return Err(DbError::RecordAbsent),
        };
        if slot.id==0{
            return Err(DbError::RecordAbsent);
        }
        if slot.offset as usize>=PAGE_SIZE{
            self.header.flags=PageFlags::Corrupted;
            return Err(DbError::CorruptedDataError);
        }
        if slot.offset<self.header.upper{
            return Err(DbError::RecordAbsent);
        }
        if slot.size==0{
            return Err(DbError::RecordAbsent);
        }
        if slot.offset as usize+ slot.size as usize > PAGE_SIZE {
            self.header.flags=PageFlags::Corrupted;
            return Err(DbError::CorruptedDataError);
        }
        let mut buf:Vec<u8>=vec![0u8;slot.size as usize];
        buf[0..slot.size as usize].copy_from_slice(&self.buffer[slot.offset as usize..slot.offset as usize+slot.size as usize]);
        Ok(buf)
    }

    pub fn write_record(&mut self,record_id:u32,buf:&[u8],size : usize)->Result<usize,DbError>{
        if matches!(self.header.flags,PageFlags::Corrupted){
            return Err(DbError::PageCorrupted);
        }
        if !self.has_space(size){
            self.compact()?;
            if !self.has_space(size){
                return Err(DbError::SpaceOver);
            }
        }
        if !self.trash.is_empty(){
            let mut offset:u16=0;
            for i in (0..self.trash.len()).rev(){
                let (slot_offset,hole_size)=&self.trash[i];
                if *hole_size as usize>=size{
                    offset=*slot_offset;
                    self.trash.remove(i);
                    break;
                }
            }
            if offset!=0{
                let id_bytes=record_id.to_le_bytes();
                self.buffer[offset as usize..offset as usize+4].copy_from_slice(&id_bytes);
                let size_bytes=(size as u32).to_le_bytes();
                self.buffer[offset as usize+6..offset as usize+10].copy_from_slice(&size_bytes);
                let offset:u16=u16::from_le_bytes(self.buffer[offset as usize+4..offset as usize+6].try_into().unwrap());
                self.buffer[offset as usize..offset as usize+size].copy_from_slice(buf);
                self.header.flags=PageFlags::Dirty;
                return Ok(size)
            }
        }
        let free_space=self.header.upper-self.header.lower;
        if (free_space as u32)<SLOT_SIZE as u32+size as u32{
            return Err(DbError::SpaceOver);
        }
        self.buffer[self.header.upper as usize-size..self.header.upper as usize].copy_from_slice(&buf);
        self.header.upper-=size as u16;
        let mut slot_bytes=[0u8;SLOT_SIZE];
        let offset:usize=self.header.upper as usize;
        let offset_bytes=(offset as u16).to_le_bytes();
        let record_id_bytes=record_id.to_le_bytes();
        let size_bytes=(size as u32).to_le_bytes();
        slot_bytes[0..4].copy_from_slice(&record_id_bytes);
        slot_bytes[4..6].copy_from_slice(&offset_bytes);
        slot_bytes[6..10].copy_from_slice(&size_bytes);
        self.buffer[self.header.lower as usize..self.header.lower as usize+SLOT_SIZE].copy_from_slice(&slot_bytes);
        self.header.item_count+=1;
        self.header.lower+=SLOT_SIZE as u16;
        self.header.flags=PageFlags::Dirty;
        Ok(size)
    }

    pub fn update_record(&mut self,record_id:u32,buf:&[u8],size:usize)->Result<usize,DbError>{
        if matches!(self.header.flags,PageFlags::Corrupted){
            return Err(DbError::PageCorrupted);
        }
        let mut offset:usize=PAGE_HEADER_SIZE;
        let mut slot : Option<Slot>=None;
        for _ in 0..self.header.item_count{
            let candidate=Slot::deserialise(&self.buffer,offset);
            if candidate.id==record_id {
                slot=Some(candidate);
                break;
            }
            offset+=SLOT_SIZE;
        }
        let slot=match slot{
            Some(s)=>s,
            None=>return Err(DbError::RecordAbsent),
        };
        if slot.size==0{
            return Err(DbError::RecordAbsent);
        }
        if slot.size>=size as u32{
            self.buffer[slot.offset as usize..slot.offset as usize+size].copy_from_slice(&buf);
            let size_bites=(size as u32).to_le_bytes();
            self.buffer[offset+6..offset+10].copy_from_slice(&size_bites);
            self.header.flags=PageFlags::Dirty;
            return Ok(size);
        }
        else{
            self.delete_record(record_id)?;
            self.compact()?;
            self.write_record(record_id,buf,size)
        }
    }

    pub fn delete_record(&mut self,record_id:u32)->Result<(),DbError>{
        if matches!(self.header.flags,PageFlags::Corrupted){
            return Err(DbError::PageCorrupted);
        }
        let mut offset:usize=PAGE_HEADER_SIZE;
        let mut slot : Option<Slot>=None;
        for _ in 0..self.header.item_count{
            let candidate=Slot::deserialise(&self.buffer,offset);
            if candidate.id==record_id {
                slot=Some(candidate);
                break;
            }
            offset+=SLOT_SIZE;
        }
        let slot=match slot{
            Some(s)=>s,
            None=>return Err(DbError::RecordAbsent),
        };
        self.trash.push((offset as u16,slot.size));
        let bytes_read=slot.size as usize;
        if bytes_read==0{
            self.header.flags=PageFlags::Corrupted;
            return Err(DbError::CorruptedDataError);
        }
        if slot.offset as usize>=PAGE_SIZE{
            self.header.flags=PageFlags::Corrupted;
            return Err(DbError::CorruptedDataError);
        }
        if slot.offset<self.header.upper{
            self.header.flags=PageFlags::Corrupted;
            return Err(DbError::CorruptedDataError);
        }
        let new_id:u32=0;
        let zero_bytes=new_id.to_le_bytes();
        self.buffer[offset+0..offset+4].copy_from_slice(&zero_bytes);
        let size=slot.size;
        let buf:Vec<u8>=vec![0u8;size as usize];
        self.buffer[slot.offset as usize..slot.offset as usize+slot.size as usize].copy_from_slice(&buf);
        drop(buf);
        self.header.flags=PageFlags::Dirty;
        Ok(())
    }

    fn compact(&mut self)->Result<(),DbError>{
        let mut new_buf=[0u8;PAGE_SIZE];
        new_buf[0..PAGE_HEADER_SIZE].copy_from_slice(&self.buffer[0..PAGE_HEADER_SIZE]); 
        let mut alive:Vec<(u32,u16,u32)>=Vec::new();
        let mut offset=PAGE_HEADER_SIZE;
        while offset<self.header.lower as usize{
            let id=u32::from_le_bytes(self.buffer[offset..offset+4].try_into().unwrap());
            if id==0{
                offset+=SLOT_SIZE;
                continue;
            }
            let record_offset=u16::from_le_bytes(self.buffer[offset+4..offset+6].try_into().unwrap());
            let record_size=u32::from_le_bytes(self.buffer[offset+6..offset+10].try_into().unwrap());
            alive.push((id,record_offset,record_size));
            offset+=SLOT_SIZE;
        }
        let n_records=alive.len();
        self.header.item_count=n_records as u16;
        self.header.flags=PageFlags::Dirty;
        self.header.lower=(n_records*SLOT_SIZE+PAGE_HEADER_SIZE) as u16;
        let mut upper:u16=8192;
        let mut lower=PAGE_HEADER_SIZE;
        loop{
            if alive.len()==0{
                break;
            }
            let (record_id,record_offset,record_size)=match alive.pop(){
                Some(tup)=>tup,
                None=>break,
            };
            if record_size==0{
                self.header.flags=PageFlags::Corrupted;
                return Err(DbError::CorruptedDataError);
            }
            if record_offset>PAGE_SIZE as u16{
                self.header.flags=PageFlags::Corrupted;
                return Err(DbError::CorruptedDataError);
            }
            let mut slot=[0u8;SLOT_SIZE];
            let new_offset:u16=upper-record_size as u16;
            let id_bytes=record_id.to_le_bytes();
            let offset_bytes=new_offset.to_le_bytes();
            let size_bytes=record_size.to_le_bytes();
            slot[0..4].copy_from_slice(&id_bytes);
            slot[4..6].copy_from_slice(&offset_bytes);
            slot[6..10].copy_from_slice(&size_bytes);
            new_buf[lower..lower+SLOT_SIZE].copy_from_slice(&slot);
            new_buf[upper as usize-record_size as usize..upper as usize].copy_from_slice(&self.buffer[record_offset as usize..record_offset as usize+record_size as usize]);
            upper-=record_size as u16;
            lower+=SLOT_SIZE;
        }
        self.buffer=new_buf;
        self.header.upper=upper;
        let x:u32=0;
        let x_bytes=x.to_le_bytes();
        self.header.serialise(&mut self.buffer);
        self.buffer[24..28].copy_from_slice(&x_bytes);
        let checksum:u32=crc32(&self.buffer);
        let checksum_bytes=checksum.to_le_bytes();
        self.buffer[24..28].copy_from_slice(&checksum_bytes);
        self.header.checksum=checksum;
        self.header.serialise(&mut self.buffer);
        self.trash.clear();
        Ok(())
    }
}

