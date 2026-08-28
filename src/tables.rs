use std::os::unix::prelude::FileExt;
use std::fs::File;
use std::vec::Vec;
use std::collections::HashMap;
use crate::db_errors::DbError;
use crate::crc32::crc32;
use crate::index::Index;

const MAGIC : u32=69420;

struct Table{
    file        :   File,
    size        :   u32,
    records     :   Vec<u32>,
    schema      :   Vec<(String,DataTypes)>,
    next_si_no  :   u32,
}

#[repr(u16)]
pub enum DataTypes{
    INT32,
    UINT32,
    FLOAT32,
    VARCHAR,
}

#[derive(PartialEq)]
pub enum Value {
    Int32(i32),
    Uint32(u32),
    Float32(f32),
    Varchar(String),
}

impl Value {
    fn compare(&self, other: &Value) -> Result<std::cmp::Ordering, DbError> {
        match (self, other) {
            (Value::Int32(a), Value::Int32(b)) => Ok(a.cmp(b)),
            (Value::Uint32(a), Value::Uint32(b)) => Ok(a.cmp(b)),
            (Value::Float32(a), Value::Float32(b)) => {
                a.partial_cmp(b).ok_or(DbError::InvalidComparison)
            }
            (Value::Varchar(a), Value::Varchar(b)) => Ok(a.cmp(b)),

            _ => Err(DbError::TypeMismatch),
        }
    }
}

pub enum Operator{
    Equal,
    NotEqual,
    Greater,
    Less,
    GreaterEqual,
    LessEqual,
}

impl Operator {
    fn matches(&self, ordering: std::cmp::Ordering) -> bool {
        use std::cmp::Ordering::*;
        match self {
            Operator::Equal        => ordering == Equal,
            Operator::NotEqual     => ordering != Equal,
            Operator::Less         => ordering == Less,
            Operator::Greater      => ordering == Greater,
            Operator::GreaterEqual => ordering != Less,
            Operator::LessEqual    => ordering != Greater,
        }
    }
}

pub struct Condition{
    column      :   String,
    operator    :   Operator,
    value       :   Value,
}

impl Table{
    pub fn new(mut name : String,schema:Vec<(String,DataTypes)>)->Result<Self,DbError>{
        name+=".table";
        let file=File::options()
            .read(true)
            .write(true)
            .create(true)
            .open(name).map_err(|_| DbError::FileError)?;
        let file_size=file.metadata().map_err(|_| DbError::FileError)?.len();
        if file_size!=0{
            return Err(DbError::TableNameExists);
        }
        let records:Vec<u32>=Vec::new();
        Ok(
            Self{
                file,
                size : 0,
                records,
                schema,
                next_si_no : 1,
            }
        )
    }

    pub fn deserialise(mut name:String)->Result<Self,DbError>{
        name+=".table";
        let file=File::options()
            .read(true)
            .write(true)
            .open(name).map_err(|_| DbError::FileError)?;
        let file_size=file.metadata().map_err(|_| DbError::FileError)?.len();
        if file_size<18{
            return Err(DbError::CorruptedDataError);
        }
        let mut offset:usize=0;
        let mut table = vec![0u8;file_size as usize];
        file.read_at(&mut table,offset as u64).map_err(|_| DbError::FileError)?;
        let magic=u32::from_le_bytes(table[0..4].try_into().unwrap());
        if magic!=MAGIC{
            return Err(DbError::CorruptedDataError);
        }
        let checksum=u32::from_le_bytes(table[4..8].try_into().unwrap());
        let size=u32::from_le_bytes(table[8..12].try_into().unwrap());
        let num_columns=u16::from_le_bytes(table[12..14].try_into().unwrap());
        let next_si_no=u32::from_le_bytes(table[14..18].try_into().unwrap());
        let z:u32=0;
        let zero_bytes=z.to_le_bytes();
        table[4..8].copy_from_slice(&zero_bytes);
        let calc_checksum=crc32(&table);
        if calc_checksum!=checksum{
            return Err(DbError::CorruptedDataError);
        }
        let checksum_bytes=checksum.to_le_bytes();
        table[4..8].copy_from_slice(&checksum_bytes);
        let mut schema:Vec<(String,DataTypes)>=Vec::new();
        offset=18;
        for _ in 0..num_columns{
            let col_name_size=u16::from_le_bytes(table[offset..offset+2].try_into().unwrap());
            offset+=2;
            if col_name_size as u64+offset as u64+2>=file_size{
                return Err(DbError::CorruptedDataError);
            }
            let mut col_name_bytes=vec![0u8;col_name_size as usize];
            col_name_bytes.copy_from_slice(&table[offset..offset+col_name_size as usize]);
            let col_name=String::from_utf8(col_name_bytes).map_err(|_| DbError::CorruptedDataError)?;
            offset+=col_name_size as usize;
            let data_type=match u16::from_le_bytes(table[offset..offset+2].try_into().unwrap()){
                0=>DataTypes::INT32,
                1=>DataTypes::UINT32,
                2=>DataTypes::FLOAT32,
                3=>DataTypes::VARCHAR,
                _=>return Err(DbError::CorruptedDataError),
            };
            offset+=2;
            schema.push((col_name,data_type));
        }
        let mut records:Vec<u32>=Vec::new();
        for _ in 0..size{
            let record_id=u32::from_le_bytes(table[offset..offset+4].try_into().unwrap());
            records.push(record_id);
            offset+=4;
        }
        Ok(
            Self{
                file,
                size,
                records,
                schema,
                next_si_no,
            }
        )
    }

    pub fn serialise(&mut self)->Result<(),DbError>{
        let file_size =18+ self.schema.iter().map(|(name, _)| 2 + name.len() + 2).sum::<usize>()+ self.records.len() * 4;
        let mut table=vec![0u8;file_size as usize];
        let num_columns=self.schema.len();
        let mut offset:usize=18;
        let magic=MAGIC;
        let magic_bytes=magic.to_le_bytes();
        self.size=self.records.len() as u32;
        let size_bytes=self.size.to_le_bytes();
        let num_columns_bytes=(num_columns as u16).to_le_bytes();
        let z:u32=0;
        let zero_bytes=z.to_le_bytes();
        let next_si_bytes=self.next_si_no.to_le_bytes();
        table[0..4].copy_from_slice(&magic_bytes);
        table[4..8].copy_from_slice(&zero_bytes);
        table[8..12].copy_from_slice(&size_bytes);
        table[12..14].copy_from_slice(&num_columns_bytes);
        table[14..18].copy_from_slice(&next_si_bytes);
        for i in 0..num_columns{
            let (column_name,data_type)=&self.schema[i];
            let column_name_bytes=column_name.as_bytes();
            let column_name_size=column_name_bytes.len();
            let cnsb=(column_name_size as u16).to_le_bytes();
            let type_:u16=match *data_type{
                DataTypes::INT32=>0,
                DataTypes::UINT32=>1,
                DataTypes::FLOAT32=>2,
                DataTypes::VARCHAR=>3,
            };
            let type_bytes=type_.to_le_bytes();
            table[offset..offset+2].copy_from_slice(&cnsb);
            offset+=2;
            table[offset..offset+column_name_size].copy_from_slice(&column_name_bytes);
            offset+=column_name_size;
            table[offset..offset+2].copy_from_slice(&type_bytes);
            offset+=2;
        }
        for rec in &self.records{
            let record_id_bytes=rec.to_le_bytes();
            table[offset..offset+4].copy_from_slice(&record_id_bytes);
            offset+=4;
        }
        if offset!=file_size{
            return Err(DbError::CorruptedDataError);
        }
        let checksum=crc32(&table);
        let checksum_bytes=checksum.to_le_bytes();
        table[4..8].copy_from_slice(&checksum_bytes);
        self.file.set_len(table.len() as u64).map_err(|_| DbError::FileError)?;
        self.file.write_all_at(&table,0).map_err(|_| DbError::FileError)?;
        self.file.sync_all().map_err(|_| DbError::FileError)?;
        Ok(())
    }

    pub fn extract(&self,buf:&[u8])->Result<Vec<(String,Value)>,DbError>{
        let mut row:Vec<(String,Value)>=Vec::new();
        let mut offset=0;
        for col in &self.schema{
            let (col_name,data_type)=col;
            let cname=col_name.clone();
            match data_type{
                DataTypes::INT32=>{
                    let val=i32::from_le_bytes(buf[offset..offset+4].try_into().unwrap());
                    row.push((cname,Value::Int32(val)));
                    offset+=4;
                },
                DataTypes::UINT32=>{
                    let val=u32::from_le_bytes(buf[offset..offset+4].try_into().unwrap());
                    row.push((cname,Value::Uint32(val)));
                    offset+=4;
                },
                DataTypes::FLOAT32=>{
                    let val=f32::from_le_bytes(buf[offset..offset+4].try_into().unwrap());
                    row.push((cname,Value::Float32(val)));
                    offset+=4;
                },
                DataTypes::VARCHAR=>{
                    let data_size=u16::from_le_bytes(buf[offset..offset+2].try_into().unwrap());
                    offset+=2;
                    let mut v=vec![0u8;data_size as usize];
                    v.copy_from_slice(&buf[offset..offset+data_size as usize]);
                    let val=String::from_utf8(v).map_err(|_| DbError::CorruptedDataError)?;
                    row.push((cname,Value::Varchar(val)));
                    offset+=data_size as usize;
                },
            };
        }
        Ok(row)
    }

    fn scan(&self, index:&mut Index, conditions : Vec<Condition>)->Result<Vec<Vec<(String,Value)>>,DbError>{
        let mut rows:Vec<Vec<(String,Value)>>=Vec::new();
        'outer:for id in &self.records{
            let buf=index.get_record(*id).map_err(|_| DbError::CorruptedDataError)?;
            let row=self.extract(&buf)?;
            let mut cols:HashMap<&String,&Value>=HashMap::new();
            for col in &row{
                let (col_name,value)=col;
                cols.insert(&col_name,value);
            }
            'inner:for condition in &conditions{
                let value:&Value=match cols.get(&condition.column){
                    Some(t)=>t,
                    None=>return Err(DbError::ColumnAbsent),
                };
                let ordering=condition.value.compare(value)?;
                if condition.operator.matches(ordering){
                    continue 'inner;
                }else{
                    continue 'outer;
                }
            }
            rows.push(row);
        }
        Ok(rows)
    }
}
