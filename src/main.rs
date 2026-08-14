use std::fs::File;
use std::io::Seek;
use std::process;
use std::os::unix::prelude::FileExt;
use std::io::SeekFrom;
use std::fmt;

const PAGE_SIZE:usize=8192;
const MAGIC : u32=69420;
const PAGE_HEADER_SIZE:usize=96;

#[derive(Debug)]
pub struct CorruptedDataError;

impl fmt::Display for CorruptedDataError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "corrupted data")
    }
}

impl std::error::Error for CorruptedDataError {}

#[repr(u16)]
enum PageType{
    Free = 0,
    Data = 1,
    BTreeLeaf=2,
    BtreeInternal=3,
    Meta=4,
}

struct DatabaseFile{
    file : std::fs::File,
    size : u64,
}

impl DatabaseFile{
    fn read_page(&self,page_id:u64,buf : &mut [u8;PAGE_SIZE])-> Result<(),std::io::Error> {
        let offset:u64=page_id*(PAGE_SIZE as u64);
        self.file.read_at(buf,offset)?;
        Ok(())
    }
    fn write_page(&self,page_id:u64,buf : &[u8;PAGE_SIZE])-> Result<(),std::io::Error> {
        let offset:u64=page_id*(PAGE_SIZE as u64);
        self.file.write_all_at(buf,offset)?;
        Ok(())
    }
    fn allocate_page(&mut self)->Result<u64,std::io::Error>{
        let offset:u64=self.size/(PAGE_SIZE as u64);
        let page_id:u64=offset;
        let header : PageHeader=PageHeader::new(page_id,PageType::Free);
        let mut buffer = [0u8;PAGE_SIZE];
        header.serialise(&mut buffer);
        self.file.write_all_at(&buffer,self.size)?;
        self.size+=PAGE_SIZE as u64;

        Ok(page_id)
    }
}

struct PageHeader{
    magic       :   u32,    //To ensure it is of this table and not corrupted
    page_id     :   u64,    //Page ID
    page_type   :   PageType,    //Meta data, B tree internals, etc
    flags       :   u16,    //Unused for now, stuff like corrupted, dirty
    lsn         :   u64,    //This comes with Write Ahead Logs
    checksum    :   u32,    //To deal with corruption
    item_count  :   u16,    //For slotted pages
    lower       :   u16,    //End of slot list -> it grows downwards
    upper       :   u16,    //Upper point of the records -> they grow upwards
    reserved    :   [u8;62],
}

impl PageHeader {
    fn new(page_id: u64, page_type: PageType) -> Self {
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

    fn serialise(&self,buffer : &mut [u8;PAGE_SIZE]){
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

    fn deserialise(buffer:&mut [u8;PAGE_SIZE])->Result<Self,CorruptedDataError>{
        let magic=u32::from_le_bytes(buffer[0..4].try_into().unwrap());
        if magic!=MAGIC{
            return Err(CorruptedDataError);
        }
        let page_id=u64::from_le_bytes(buffer[4..12].try_into().unwrap());
        let page_type=match u16::from_le_bytes(buffer[12..14].try_into().unwrap()){
            0=>PageType::Free,
            1=>PageType::Data,
            2=>PageType::BTreeLeaf,
            3=>PageType::BtreeInternal,
            4=>PageType::Meta,
            _=>return Err(CorruptedDataError),
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

fn main() {
    let mut file=match File::options()
        .write(true)
        .read(true)
        .create(true)
        .open("database.db")
    {
        Ok(f)=>f,
        Err(e)=>{
            eprintln!("Failed to open file : {e}");
            process::exit(1);
        },
    };
    let fsize=match file.seek(SeekFrom::End(0)){
        Ok(s)=>s,
        Err(e)=>{
            eprintln!("Couldn't read file size : {e}");
            process::exit(1);
        }
    };
    match file.seek(SeekFrom::Start(0)){
        Err(e)=>{
            eprintln!("Couldn't find file head : {e}");
            process::exit(1);
        }
        _=>{}
    };
    let mut db_file : DatabaseFile=DatabaseFile{file : file,size : fsize};
    let page_id = db_file.allocate_page().unwrap();

    let mut buffer = [0u8; PAGE_SIZE];

    db_file.read_page(page_id, &mut buffer).unwrap();

    let header = PageHeader::deserialise(&mut buffer).unwrap();

    println!("page id: {}", header.page_id);
    println!("magic: {}", header.magic);
    println!("item count: {}", header.item_count);
    println!("lower: {}", header.lower);
    println!("upper: {}", header.upper);
}
