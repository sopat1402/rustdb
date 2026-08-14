mod page;
use std::fs::File;
use std::io::Seek;
use std::process;
use std::io::SeekFrom;
use page::DatabaseFile;
use page::PAGE_SIZE;
use page::PageHeader;

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
