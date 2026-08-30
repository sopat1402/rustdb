use std::os::unix::prelude::FileExt;
use std::collections::HashSet;
use std::fs::File;
use std::vec::Vec;
use std::collections::HashMap;
use crate::db_errors::DbError;
use crate::table_wal::{TableWAL,TaskType};
use crate::crc32::crc32;
use crate::index::Index;

const MAGIC : u32=69420;

pub struct Tables{
    file    :   File,
    tables  :   HashMap<String,Table>,
    index   :   Index,
    wal     :   TableWAL,
}

#[derive(Copy,Clone)]
#[repr(u16)]
pub enum DataTypes{
    INT32,
    UINT32,
    FLOAT32,
    VARCHAR,
}

#[derive(PartialEq,Debug,Clone)]
pub enum Value {
    Int32(i32),
    Uint32(u32),
    Float32(f32),
    Varchar(String),
}

impl Tables{
    pub fn bootup()->Result<Self,DbError>{
        let file=File::options()
            .read(true)
            .write(true)
            .create(true)
            .open("tables.tables").map_err(|_| DbError::FileError)?;
        let size=file.metadata().map_err(|_| DbError::FileError)?.len();
        let mut tables:HashMap<String,Table>=HashMap::new();
        let index=Index::bootup()?;
        let mut file_buff=vec![0u8;size as usize];
        if size!=0{
            file.read_at(&mut file_buff,0).map_err(|_| DbError::FileError)?;
            let mut offset:usize=0;
            while offset<size as usize{
                let table_name_size=u16::from_le_bytes(file_buff[offset..offset+2]
                    .try_into()
                    .map_err(|_| DbError::CorruptedDataError)?
                );
                let mut name_buf=vec![0u8;table_name_size as usize];
                offset+=2;
                name_buf.copy_from_slice(&file_buff[offset..offset+table_name_size as usize]);
                let name=String::from_utf8(name_buf).map_err(|_| DbError::CorruptedDataError)?;
                offset+=table_name_size as usize;
                let table=Table::deserialise(&name)?;
                tables.insert(name,table);
            }
        }
        let wal_file=File::options()
            .read(true)
            .write(true)
            .create(true)
            .open("tables.log").map_err(|_| DbError::FileError)?;
        let size=wal_file.metadata().map_err(|_| DbError::FileError)?.len();
        if size==0{
            let len:u16=0;
            let last_lsn:u64=0;
            let len_bytes=len.to_le_bytes();
            let lsn_bytes=last_lsn.to_le_bytes();
            let mut write_buf:Vec<u8>=Vec::new();
            write_buf.extend(lsn_bytes);
            write_buf.extend(len_bytes);
        }
        else if size<10{
            return Err(DbError::CorruptedWAL);
        }
        let wal=TableWAL::deserialise(wal_file)?;
        Ok(
            Self{
                file,
                tables,
                index,
                wal,
            }
        )
    }

    pub fn create_table(&mut self,name:String,schema:Vec<(String,DataTypes)>)->Result<(),DbError>{
        let n=name.clone();
        let table=Table::new(&name,schema,self.wal.last_lsn)?;
        self.tables.insert(name,table);
        let mut size=self.file.metadata().map_err(|_| DbError::FileError)?.len();
        let name_size=n.len();
        let mut name_buf=n.as_bytes();
        let mut name_size_bytes=(name_size as u16).to_le_bytes();
        self.file.write_all_at(&mut name_size_bytes,size).map_err(|_| DbError::FileError)?;
        size+=2;
        self.file.write_all_at(&mut name_buf,size).map_err(|_| DbError::FileError)?;
        self.file.sync_all().map_err(|_| DbError::FileError)?;
        Ok(())
    }

    pub fn get_schema(&self,name:Option<String>)->Result<Vec<(String,DataTypes)>,DbError>{
        let name=name.ok_or(DbError::TableAbsent)?;
        let table:&Table=match self.tables.get(&name){
            Some(t)=>t,
            None=>return Err(DbError::TableAbsent),
        };
        let s=table.schema.clone();
        Ok(s)
    }

    pub fn delete_table(&mut self,name:String)->Result<(),DbError>{
        let table:&mut Table=match self.tables.get_mut(&name){
            Some(t)=>t,
            None=>return Err(DbError::TableAbsent),
        };
        table.delete_table(&mut self.index,&name)?;
        let _=self.tables.remove(&name);
        let tables=self.tables.keys();
        let mut write_buf:Vec<u8>=Vec::new();
        for table_name in tables{
            let length=table_name.len();
            let s=table_name.clone();
            let name_bytes=s.as_bytes();
            let length_bytes=(length as u16).to_le_bytes();
            write_buf.extend(length_bytes);
            write_buf.extend(name_bytes);
        }
        self.file.set_len(write_buf.len() as u64).map_err(|_| DbError::FileError)?;
        self.file.write_all_at(&mut write_buf,0).map_err(|_| DbError::FileError)?;
        self.file.sync_all().map_err(|_| DbError::FileError)?;
        Ok(())
    }

    pub fn insert(&mut self,table_name:&String,row:Vec<(String,Value)>)->Result<(),DbError>{
        let table=match self.tables.get_mut(table_name){
            Some(t)=>t,
            None=>return Err(DbError::TableAbsent),
        };
        let id=table.insert(&mut self.index,row)?;
        self.wal.add_log(TaskType::Insert,id,table_name)?;
        table.lsn=self.wal.last_lsn;
        self.checkpoint(false)?;
        Ok(())
    }

    pub fn select(&mut self,table_name:&String,conditions:Vec<Condition>,cols:Vec<String>)->Result<Vec<Vec<(String,Value)>>,DbError>{
        let table=match self.tables.get_mut(table_name){
            Some(t)=>t,
            None=>return Err(DbError::TableAbsent),
        };
        let res=table.select(&mut self.index,conditions,cols)?;
        Ok(res)
    }

    pub fn update(&mut self, table_name:&String,conditions:Vec<Condition>,updates:Vec<Condition>)->Result<usize,DbError>{
        let table=match self.tables.get_mut(table_name){
            Some(t)=>t,
            None=>return Err(DbError::TableAbsent),
        };
        let updated=table.update(&mut self.index,conditions,updates)?;
        self.checkpoint(false)?;
        Ok(updated)
    }

    pub fn delete(&mut self,table_name:&String,conditions:Vec<Condition>)->Result<usize,DbError>{
        let table=match self.tables.get_mut(table_name){
            Some(t)=>t,
            None=>return Err(DbError::TableAbsent),
        };
        let deleted=table.delete(&mut self.index,conditions)?;
        for id in &deleted{
            self.wal.add_log(TaskType::Delete,*id,table_name)?;
        }
        table.lsn=self.wal.last_lsn;
        self.checkpoint(false)?;
        Ok(deleted.len())
    }

    pub fn shutdown(&mut self)->Result<(),DbError>{
        self.checkpoint(true)?;
        let tables=self.tables.values_mut();
        for table in tables{
            table.serialise()?;
        }
        self.index.shutdown()?;
        self.wal.reset()?;
        Ok(())
    }

    pub fn checkpoint(&mut self,force:bool)->Result<(),DbError>{
        if force || self.index.wal.file_size==10{
            let mut entry=self.wal.get_log_any(None)?;
            let mut modded:HashSet<String>=HashSet::new();
            while let Some((log,iterator))=entry{
                let table_name=log.table_name;
                let table=match self.tables.get_mut(&table_name){
                    Some(t)=>t,
                    None=>{
                        entry=self.wal.get_log_any(Some(iterator))?;
                        continue;
                    },
                };
                if table.lsn>=log.lsn{
                    entry=self.wal.get_log_any(Some(iterator))?;
                    continue;
                }else{
                    match log.task_type{
                        TaskType::Insert=>{
                            table.records.push(log.record_id);
                            table.lsn=log.lsn;
                            modded.insert(table_name);
                        },
                        TaskType::Delete=>{
                             if let Some(pos) = table.records.iter().position(|r_id| *r_id == log.record_id) {
                                table.records.remove(pos);
                            }
                            table.lsn=log.lsn;
                            modded.insert(table_name);
                        },
                    };
                    entry=self.wal.get_log_any(Some(iterator))?;
                    continue;
                }
            }
            for table_name in modded{
                let table=self.tables.get_mut(&table_name).unwrap();
                table.serialise()?;
            }
            self.wal.reset()?;
        }
        Ok(())
    }

    pub fn recover(&mut self)->Result<(),DbError>{
        if self.wal.file_size==10{
            return Ok(());
        }
        else if self.wal.file_size<10{
            return Err(DbError::CorruptedWAL);
        }
        else{
            let mut entry=self.wal.get_log_any(None)?;
            let mut modded:HashSet<String>=HashSet::new();
            while let Some((log,iterator))=entry{
                let table_name=log.table_name;
                let table=match self.tables.get_mut(&table_name){
                    Some(t)=>t,
                    None=>{
                        entry=self.wal.get_log_any(Some(iterator))?;
                        continue;
                    },
                };
                if table.lsn>=log.lsn{
                    entry=self.wal.get_log_any(Some(iterator))?;
                    continue;
                }else{
                    match log.task_type{
                        TaskType::Insert=>{
                            table.records.push(log.record_id);
                            table.lsn=log.lsn;
                            modded.insert(table_name);
                        },
                        TaskType::Delete=>{
                             if let Some(pos) = table.records.iter().position(|r_id| *r_id == log.record_id) {
                                table.records.remove(pos);
                            }
                            table.lsn=log.lsn;
                            modded.insert(table_name);
                        },
                    };
                    entry=self.wal.get_log_any(Some(iterator))?;
                    continue;
                }
            }
            for table_name in modded{
                let table=self.tables.get_mut(&table_name).unwrap();
                table.serialise()?;
            }
            self.wal.reset()?;
        }
        Ok(())
    }
}

struct Table{
    file            :   File,
    size            :   u32,
    records         :   Vec<u32>,
    schema          :   Vec<(String,DataTypes)>,
    next_si_no      :   u32,
    lsn             :   u64,
}



impl Value {
    fn compare(&self, other: &Value) -> Result<std::cmp::Ordering, DbError> {
        match (self, other) {
            (Value::Int32(a), Value::Int32(b)) => {
                Ok(a.cmp(b))
            },
            (Value::Uint32(a), Value::Uint32(b)) =>{
                Ok(a.cmp(b))
            },
            (Value::Float32(a), Value::Float32(b)) => {
                a.partial_cmp(b).ok_or(DbError::InvalidComparison)
            }
            (Value::Varchar(a), Value::Varchar(b)) => Ok(a.cmp(b)),

            _ => Err(DbError::TypeMismatch),
        }
    }
}

#[derive(Clone)]
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

#[derive(Clone)]
pub struct Condition{
    pub column      :   String,
    pub operator    :   Operator,
    pub value       :   Value,
}

impl Table{
    fn new(name : &String,schema:Vec<(String,DataTypes)>,lsn:u64)->Result<Self,DbError>{
        let mut f_name=name.clone();
        f_name+=".table";
        let file=File::options()
            .read(true)
            .write(true)
            .create(true)
            .open(f_name).map_err(|_| DbError::FileError)?;
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
                lsn,
            }
        )
    }

    fn deserialise(name:&String)->Result<Self,DbError>{
        let mut name=name.clone();
        name+=".table";
        let file=File::options()
            .read(true)
            .write(true)
            .open(name).map_err(|_| DbError::FileError)?;
        let file_size=file.metadata().map_err(|_| DbError::FileError)?.len();
        if file_size<26{
            return Err(DbError::CorruptedDataError);
        }
        let mut offset:usize=0;
        let mut table = vec![0u8;file_size as usize];
        file.read_at(&mut table,offset as u64).map_err(|_| DbError::FileError)?;
        let magic=u32::from_le_bytes(table[0..4].try_into().map_err(|_| DbError::CorruptedDataError)?);
        if magic!=MAGIC{
            return Err(DbError::CorruptedDataError);
        }
        let checksum=u32::from_le_bytes(table[4..8].try_into().map_err(|_| DbError::CorruptedDataError)?);
        let size=u32::from_le_bytes(table[8..12].try_into().map_err(|_| DbError::CorruptedDataError)?);
        let num_columns=u16::from_le_bytes(table[12..14].try_into().map_err(|_| DbError::CorruptedDataError)?);
        let next_si_no=u32::from_le_bytes(table[14..18].try_into().map_err(|_| DbError::CorruptedDataError)?);
        let lsn=u64::from_le_bytes(table[18..26].try_into().map_err(|_| DbError::CorruptedDataError)?);
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
        offset=26;
        for _ in 0..num_columns{
            let col_name_size=u16::from_le_bytes(table[offset..offset+2].try_into().map_err(|_| DbError::CorruptedDataError)?);
            offset+=2;
            if col_name_size as u64+offset as u64+2>=file_size{
                return Err(DbError::CorruptedDataError);
            }
            let mut col_name_bytes=vec![0u8;col_name_size as usize];
            col_name_bytes.copy_from_slice(&table[offset..offset+col_name_size as usize]);
            let col_name=String::from_utf8(col_name_bytes).map_err(|_| DbError::CorruptedDataError)?;
            offset+=col_name_size as usize;
            let data_type=match u16::from_le_bytes(table[offset..offset+2].try_into().map_err(|_| DbError::CorruptedDataError)?){
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
            let record_id=u32::from_le_bytes(table[offset..offset+4].try_into().map_err(|_| DbError::CorruptedDataError)?);
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
                lsn,
            }
        )
    }

    fn serialise(&mut self)->Result<(),DbError>{
        let file_size =26 + self.schema.iter().map(|(name, _)| 2 + name.len() + 2).sum::<usize>()+ self.records.len() * 4;
        let mut table=vec![0u8;file_size as usize];
        let num_columns=self.schema.len();
        let mut offset:usize=26;
        let magic=MAGIC;
        let magic_bytes=magic.to_le_bytes();
        let lsn_bytes=self.lsn.to_le_bytes();
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
        table[18..26].copy_from_slice(&lsn_bytes);
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

    fn extract(&self,buf:&[u8])->Result<Vec<(String,Value)>,DbError>{
        let mut row:Vec<(String,Value)>=Vec::new();
        let si=u32::from_le_bytes(buf[0..4].try_into().map_err(|_| DbError::CorruptedDataError)?);
        row.push((String::from("SI"),Value::Uint32(si)));
        let mut offset=4;
        for col in &self.schema{
            let (col_name,data_type)=col;
            let cname=col_name.clone();
            match data_type{
                DataTypes::INT32=>{
                    let val=i32::from_le_bytes(buf[offset..offset+4].try_into().map_err(|_| DbError::CorruptedDataError)?);
                    row.push((cname,Value::Int32(val)));
                    offset+=4;
                },
                DataTypes::UINT32=>{
                    let val=u32::from_le_bytes(buf[offset..offset+4].try_into().map_err(|_| DbError::CorruptedDataError)?);
                    row.push((cname,Value::Uint32(val)));
                    offset+=4;
                },
                DataTypes::FLOAT32=>{
                    let val=f32::from_le_bytes(buf[offset..offset+4].try_into().map_err(|_| DbError::CorruptedDataError)?);
                    row.push((cname,Value::Float32(val)));
                    offset+=4;
                },
                DataTypes::VARCHAR=>{
                    let data_size=u16::from_le_bytes(buf[offset..offset+2].try_into().map_err(|_| DbError::CorruptedDataError)?);
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

    fn row_to_bytes(row : Vec<(String,Value)>)->Vec<u8>{
        let mut v:Vec<u8>=Vec::new();
        for (_,val) in row{
            match val{
                Value::Int32(value)=>{
                    let v_bytes=value.to_le_bytes();
                    v.extend(v_bytes);
                },
                Value::Uint32(value)=>{
                    let v_bytes=value.to_le_bytes();
                    v.extend(v_bytes);
                },
                Value::Float32(value)=>{
                    let v_bytes=value.to_le_bytes();
                    v.extend(v_bytes);
                },
                Value::Varchar(value)=>{
                    let length=value.len() as u16;
                    let length_bytes=length.to_le_bytes();
                    v.extend(length_bytes);
                    let v_bytes=value.as_bytes();
                    v.extend(v_bytes);
                },
            };
        }
        v
    }

    fn scan(&mut self, index:&mut Index, conditions : Vec<Condition>)->Result<Vec<(Vec<(String,Value)>,u32)>,DbError>{
        let mut rows:Vec<(Vec<(String,Value)>,u32)>=Vec::new();
        let mut fucked:Vec<u32>=Vec::new();
        'outer:for id in &self.records{
            let buf=match index.get_record(*id).map_err(|_| DbError::CorruptedDataError){
                Ok(v)=>v,
                Err(DbError::RecordAbsent)=>{
                    fucked.push(*id);
                    continue 'outer;
                },
                Err(e)=>return Err(e),
            };
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
                let ordering=value.compare(&condition.value)?;
                if condition.operator.matches(ordering){
                    continue 'inner;
                }else{
                    continue 'outer;
                }
            }
            rows.push((row,*id));
        }
        for id in fucked{
            if let Some(pos) = self.records.iter().position(|r_id| *r_id == id) {
                self.records.remove(pos);
            }
        }
        Ok(rows)
    }

    fn select(&mut self,index:&mut Index, conditions : Vec<Condition>,mut cols : Vec<String>)->Result<Vec<Vec<(String,Value)>>,DbError>{
        let rows=self.scan(index,conditions)?;
        let mut result:Vec<Vec<(String,Value)>>=Vec::new();
        if cols.len()==0{
            for (col_name,_) in &self.schema{
                cols.push(col_name.clone());
            }
            cols.push(String::from("SI"));
        }
        let cols: HashSet<String> = cols.into_iter().collect();
        for (row,_) in rows{
            let mut r_row:Vec<(String,Value)>=Vec::new();
            for col in row{
                if cols.contains(&col.0){
                    r_row.push(col);
                }
            }
            result.push(r_row);
        }
        Ok(result)
    }

    fn delete(&mut self,index:&mut Index,conditions:Vec<Condition>)->Result<Vec<u32>,DbError>{
        let rows=self.scan(index,conditions)?;
        let mut deletions:Vec<u32>=Vec::new();
        for (_,id) in rows{
            index.delete_record(id)?;
            deletions.push(id);
            if let Some(pos) = self.records.iter().position(|r_id| *r_id == id) {
                self.records.remove(pos);
            }
        }
        Ok(deletions)
    }

    fn update(&mut self,index:&mut Index,conditions:Vec<Condition>,updates:Vec<Condition>)->Result<usize,DbError>{
        let rows=self.scan(index,conditions)?;
        let updated=rows.len();
        for (row,record_id) in rows{
            let mut new_row:Vec<(String,Value)>=Vec::new();
            let mut si:Value=Value::Uint32(0);
            for (col_name,value) in row{
                if col_name=="SI"{
                    si=value.clone();
                    continue;
                }
                let mut new_value=value;
                for update in &updates{
                    if update.column==col_name{
                        let type_matches=match (&new_value,&update.value){
                            (Value::Int32(_),Value::Int32(_))=>true,
                            (Value::Uint32(_),Value::Uint32(_))=>true,
                            (Value::Float32(_),Value::Float32(_))=>true,
                            (Value::Varchar(_),Value::Varchar(_))=>true,
                            _=>false,
                        };
                        if !type_matches{
                            return Err(DbError::TypeMismatch);
                        }
                        new_value=match &update.value{
                            Value::Int32(v)=>Value::Int32(*v),
                            Value::Uint32(v)=>Value::Uint32(*v),
                            Value::Float32(v)=>Value::Float32(*v),
                            Value::Varchar(v)=>Value::Varchar(v.clone()),
                        };
                    }
                }
                new_row.push((col_name,new_value));
            }
            let mut nr:Vec<(String,Value)>=Vec::new();
            nr.push((String::from("SI"),si));
            nr.extend(new_row);
            let buf=Self::row_to_bytes(nr);
            index.update_record(record_id,&buf,buf.len())?;
        }
        Ok(updated)
    }
    fn insert(&mut self, index:&mut Index,row : Vec<(String,Value)>)->Result<u32,DbError>{
        let schema: HashMap<&String, DataTypes> =self.schema.iter().map(|(name, ty)| (name, *ty)).collect();
        let len=self.schema.len() as usize;
        let mut count:usize=0;
        for col in &row{
            let (name,value)=col;
            if schema.contains_key(&name){
                let types;
                let mut _vals=(0,0,0,0);
                match value{
                    Value::Int32(_)=>{
                        _vals=(1,0,0,0);
                    },
                    Value::Uint32(_)=>{
                        _vals=(0,1,0,0);
                    },
                    Value::Float32(_)=>{
                        _vals=(0,0,1,0);
                    },
                    Value::Varchar(_)=>{
                        _vals=(0,0,0,1);
                    },
                };
                let d_type:&DataTypes=match schema.get(&name){
                    Some(t)=>t,
                    None=>return Err(DbError::CorruptedDataError),
                };
                match *d_type{
                    DataTypes::INT32=>types=(1,0,0,0),
                    DataTypes::UINT32=>types=(0,1,0,0),
                    DataTypes::FLOAT32=>types=(0,0,1,0),
                    DataTypes::VARCHAR=>types=(0,0,0,1),
                };
                if !matches!(types,_vals){
                    return Err(DbError::TypeMismatch);
                }else{
                    count+=1;
                }
            }else{
                return Err(DbError::ColumnAbsent);
            }
        }
        if count!=len{
            return Err(DbError::InsufficientParams);
        }
        let w_buf=Self::row_to_bytes(row);
        let si_bytes=self.next_si_no.to_le_bytes();
        let mut buf:Vec<u8>=Vec::new();
        buf.extend(si_bytes);
        buf.extend(w_buf);
        let _=match index.write_record(&buf,buf.len()){
            Ok(v)=>{
                self.records.push(v);
                self.next_si_no+=1;
                return Ok(v);
            },
            Err(e)=>return Err(e),
        };
    }

    fn delete_table(&mut self,index : &mut Index,name:&String)->Result<(),DbError>{
        for id in &self.records{
            index.delete_record(*id)?;
        }
        let table_file=name.clone()+".table";
        let log_file=name.clone()+".log";
        std::fs::remove_file(table_file).map_err(|_| DbError::FileError)?;
        std::fs::remove_file(log_file).map_err(|_| DbError::FileError)?;
        Ok(())
    }
}
