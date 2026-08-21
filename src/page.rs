//Code by Sohum Pathak
//sohum.pathak@protonmail.com
use std::os::unix::prelude::FileExt;
use crate::db_errors::DbError;

pub const PAGE_SIZE:usize=8192;
pub const MAGIC : u32=69420;
pub const PAGE_HEADER_SIZE:usize=96;

#[repr(u16)]
pub enum PageType{
    Free = 0,
    Data = 1,
    BTreeLeaf=2,
    BtreeInternal=3,
    Meta=4,
}

pub struct DatabaseFile{
    pub file            :   std::fs::File,
    pub page_metadata   :   std::fs::File,
    pub btree           :   std::fs::File,
    pub size            :   u64,
}

impl DatabaseFile{
    pub fn read_page(&self,page_id:u64,buf : &mut [u8;PAGE_SIZE])-> Result<(),DbError> {
        let offset:u64=page_id*(PAGE_SIZE as u64);
        if offset+PAGE_SIZE as u64>self.size{
            return Err(DbError::PageAbsent);
        }
        match self.file.read_at(buf,offset){
            Ok(_)=>return Ok(()),
            Err(_)=>return Err(DbError::PageAbsent),
        };
    }
    pub fn write_page(&self,page_id:u64,buf : &[u8;PAGE_SIZE])-> Result<(),DbError> {
        let offset:u64=page_id*(PAGE_SIZE as u64);
        let _=match self.file.write_all_at(buf,offset){
            Ok(_)=>{},
            Err(_)=>return Err(DbError::FileError),
        };
        Ok(())
    }
    pub fn allocate_page(&mut self)->Result<u64,DbError>{
        let offset:u64=self.size/(PAGE_SIZE as u64)+1;
        let page_id:u64=offset;
        let header : PageHeader=PageHeader::new(page_id,PageType::Free);
        let mut buffer = [0u8;PAGE_SIZE];
        header.serialise(&mut buffer);
        let _=match self.file.write_all_at(&buffer,self.size){
            Ok(_)=>{},
            Err(_)=>return Err(DbError::FileError),
        };
        self.size+=PAGE_SIZE as u64;

        Ok(page_id)
    }
    pub fn edit_page_metadata(&mut self,page_id:u64,new_size:usize)->Result<(),DbError>{
        let offset:u64=(page_id-1)*10;
        let mut write_buf=[0u8;10];
        let page_id_bytes=page_id.to_le_bytes();
        let size_bytes=(new_size as u16).to_le_bytes();
        write_buf[0..8].copy_from_slice(&page_id_bytes);
        write_buf[8..10].copy_from_slice(&size_bytes);
        let _=match self.page_metadata.write_all_at(&write_buf,offset){
            Ok(_)=>{},
            Err(_)=>return Err(DbError::FileError),
        };
        Ok(())
    }
}

pub struct PageHeader{
    pub magic       :   u32,    //To ensure it is of this table and not corrupted
    pub page_id     :   u64,    //Page ID
    pub page_type   :   PageType,    //Meta data, B tree internals, etc
    pub flags       :   u16,    //Unused for now, stuff like corrupted, dirty
    pub lsn         :   u64,    //This comes with Write Ahead Logs
    pub checksum    :   u32,    //To deal with corruption
    pub item_count  :   u16,    //For slotted pages
    pub lower       :   u16,    //End of slot list -> it grows downwards
    pub upper       :   u16,    //Upper point of the records -> they grow upwards
    pub reserved    :   [u8;62],
}

impl PageHeader {
    pub fn new(page_id: u64, page_type: PageType) -> Self {
        Self {
            magic: MAGIC,
            page_id,
            page_type: page_type,
            flags: 0,
            lsn: 0,
            checksum: 0,
            item_count: 0,
            lower: PAGE_HEADER_SIZE as u16,
            upper: PAGE_SIZE as u16,
            reserved: [0; 62],
        }
    }

    pub fn serialise(&self,buffer : &mut [u8;PAGE_SIZE]){
        let mut header_offset=0;

        let magic_bytes=self.magic.to_le_bytes();
        buffer[header_offset..header_offset+4].copy_from_slice(&magic_bytes);
        header_offset+=4;

        let page_id_bytes=self.page_id.to_le_bytes();
        buffer[header_offset..header_offset+8].copy_from_slice(&page_id_bytes);
        header_offset+=8;
        let typ:u16=match self.page_type{
            PageType::Free=>0,
            PageType::Data=>1,
            PageType::BTreeLeaf=>2,
            PageType::BtreeInternal=>3,
            PageType::Meta=>4,
        };
        let page_type_bytes=typ.to_le_bytes();
        buffer[header_offset..header_offset+2].copy_from_slice(&page_type_bytes);
        header_offset+=2;

        let flags_bytes=self.flags.to_le_bytes();
        buffer[header_offset..header_offset+2].copy_from_slice(&flags_bytes);
        header_offset+=2;

        let lsn_bytes=self.lsn.to_le_bytes();
        buffer[header_offset..header_offset+8].copy_from_slice(&lsn_bytes);
        header_offset+=8;

        let checksum_bytes=self.checksum.to_le_bytes();
        buffer[header_offset..header_offset+4].copy_from_slice(&checksum_bytes);
        header_offset+=4;

        let item_count_bytes=self.item_count.to_le_bytes();
        buffer[header_offset..header_offset+2].copy_from_slice(&item_count_bytes);
        header_offset+=2;

        let lower_bytes=self.lower.to_le_bytes();
        buffer[header_offset..header_offset+2].copy_from_slice(&lower_bytes);
        header_offset+=2;

        let upper_bytes=self.upper.to_le_bytes();
        buffer[header_offset..header_offset+2].copy_from_slice(&upper_bytes);
        header_offset+=2;
        buffer[header_offset..header_offset+62].copy_from_slice(&self.reserved);
    }

    pub fn deserialise(buffer:&mut [u8;PAGE_SIZE])->Result<Self,DbError>{
        let magic=u32::from_le_bytes(buffer[0..4].try_into().unwrap());
        if magic!=MAGIC{
            return Err(DbError::CorruptedDataError);
        }
        let page_id=u64::from_le_bytes(buffer[4..12].try_into().unwrap());
        let page_type=match u16::from_le_bytes(buffer[12..14].try_into().unwrap()){
            0=>PageType::Free,
            1=>PageType::Data,
            2=>PageType::BTreeLeaf,
            3=>PageType::BtreeInternal,
            4=>PageType::Meta,
            _=>return Err(DbError::CorruptedDataError),
        };
        let flags=u16::from_le_bytes(buffer[14..16].try_into().unwrap());
        let lsn=u64::from_le_bytes(buffer[16..24].try_into().unwrap());
        let checksum=u32::from_le_bytes(buffer[24..28].try_into().unwrap());
        let item_count=u16::from_le_bytes(buffer[28..30].try_into().unwrap());
        let lower=u16::from_le_bytes(buffer[30..32].try_into().unwrap());
        let upper=u16::from_le_bytes(buffer[32..34].try_into().unwrap());
        let mut reserved = [0u8;62];
        reserved.copy_from_slice(&buffer[34..96]);
        let header : PageHeader=PageHeader{
            magic,
            page_id,
            page_type,
            flags,
            lsn,
            checksum,
            item_count,
            lower,
            upper,
            reserved,
        };
        Ok(header)
    }
}

