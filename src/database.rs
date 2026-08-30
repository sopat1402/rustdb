use crate::parser::{parse,Query,QueryOperation,peek_table_name};
use crate::tables::{Tables,Value,DataTypes};
use crate::db_errors::DbError;
use std::path::Path;
use std::fs::{create_dir,remove_dir};
use std::env::set_current_dir;
use tokio::sync::{mpsc,oneshot};
use std::process::Command;

const QUEUE_CAPACITY:usize=800;

struct Job{
    query       :   String,
    respond_to  :   oneshot::Sender<Result<QueryResult, DbError>>
}

pub struct Database{
    tables      :   Tables,
    name        :   String,
    rx          :   mpsc::Receiver<Job>,
}

#[derive(Clone)]
pub struct DatabaseHandle{
    tx: mpsc::Sender<Job>,
}

pub enum QueryResult{
    Success,
    Killed,
    Count(usize),
    Rows(Vec<Vec<(String,Value)>>),
    Schema(Vec<(String,DataTypes)>),
}

impl DatabaseHandle{
    pub async fn submit_job(&self, query: String) -> Result<QueryResult, DbError>{
        let (resp_tx, resp_rx) = oneshot::channel();
        let job = Job{ query, respond_to: resp_tx };
        self.tx.send(job).await.map_err(|_| DbError::QueueClosed)?;
        resp_rx.await.map_err(|_| DbError::QueueClosed)?
    }
}

impl Database{
    pub fn new(db_name:String)->Result<(Self, DatabaseHandle),DbError>{
        let path=Path::new(&db_name);
        if path.is_dir(){
            return Err(DbError::DBExists);
        }
        create_dir(&db_name).map_err(|_| DbError::FileError)?;
        set_current_dir(&db_name).map_err(|_| DbError::FileError)?;
        let tables=Tables::bootup()?;
        let (tx, rx) = mpsc::channel(QUEUE_CAPACITY);
        Ok((
            Self{
                tables,
                name:db_name,
                rx
            },
            DatabaseHandle{tx}
        ))
    }

    pub fn shutdown(&mut self) -> Result<(), DbError>{
        self.tables.shutdown()?;
        set_current_dir("..").map_err(|_| DbError::FileError)?;
        Ok(())
    }

    pub fn bootup(db_name:String)->Result<(Self, DatabaseHandle),DbError>{
        let path=Path::new(&db_name);
        if !path.is_dir(){
            return Err(DbError::DBAbsent);
        }
        set_current_dir(&db_name).map_err(|_| DbError::FileError)?;
        let mut tables=Tables::bootup()?;
        tables.recover()?;
        let (tx, rx) = mpsc::channel(QUEUE_CAPACITY);
        Ok((
            Self{
                tables,
                name:db_name,
                rx
            },
            DatabaseHandle{tx}
        ))
    }

    pub async fn run(&mut self){
        while let Some(job) = self.rx.recv().await{
            let result = self.execute(job.query);
            let killed=matches!(result,Ok(QueryResult::Killed));
            let _ = job.respond_to.send(result);
            if killed{
                break;
            }
        }
    }

    fn execute(&mut self, query_str:String)->Result<QueryResult,DbError>{
        let table_name=peek_table_name(&query_str)?;
        let schema:Option<Vec<(String,DataTypes)>>=match self.tables.get_schema(table_name){
            Ok(v)=>Some(v),
            Err(DbError::TableAbsent)=>None,
            Err(e)=>return Err(e),
        };
        let parse_schema: Option<&Vec<(String,DataTypes)>> = schema.as_ref();
        let query:Query=parse(query_str,parse_schema)?;
        match query.task{
            QueryOperation::Shutdown=>{
                self.shutdown()?;
                return Ok(QueryResult::Killed);
            },
            QueryOperation::DeleteTable=>{
                let table_name=query.table_name.ok_or(DbError::InsufficientParams)?;
                self.tables.delete_table(table_name)?;
                return Ok(QueryResult::Success);
            },
            QueryOperation::CreateTable=>{
                let table_name=query.table_name.ok_or(DbError::InsufficientParams)?;
                let schema=query.schema.ok_or(DbError::InsufficientParams)?;
                self.tables.create_table(table_name,schema)?;
                return Ok(QueryResult::Success);
            },
            QueryOperation::CreateDB=>{
                set_current_dir("..").map_err(|_| DbError::FileError)?;
                let db_name=query.db_name.ok_or(DbError::InsufficientParams)?;
                Self::new(db_name)?;
                let mut route=String::from("../");
                route.push_str(self.name.as_str());
                set_current_dir(&route).map_err(|_| DbError::FileError)?;
                return Ok(QueryResult::Success);
            },
            QueryOperation::DropDB=>{
                let db_name=query.db_name.ok_or(DbError::InsufficientParams)?;
                let is_self=db_name==self.name;

                let mut route=String::from("../");
                route.push_str(db_name.as_str());
                let path=Path::new(&route);
                if !path.is_dir(){
                    return Err(DbError::DBAbsent);
                }
                std::fs::remove_dir_all(&route).map_err(|_| DbError::FileError)?;
                if is_self{
                    set_current_dir("..").map_err(|_| DbError::FileError)?;
                    return Ok(QueryResult::Killed);
                } else {
                    return Ok(QueryResult::Success);
                }
            },
            QueryOperation::Insert=>{
                let table_name=query.table_name.ok_or(DbError::InsufficientParams)?;
                let row=query.row.ok_or(DbError::InsufficientParams)?;
                self.tables.insert(&table_name,row)?;
                return Ok(QueryResult::Success);
            },
            QueryOperation::Update=>{
                let table_name=query.table_name.ok_or(DbError::InsufficientParams)?;
                let conditions=query.conditions.ok_or(DbError::InsufficientParams)?;
                let updates=query.updates.ok_or(DbError::InsufficientParams)?;
                let c=self.tables.update(&table_name,conditions,updates)?;
                return Ok(QueryResult::Count(c));
            },
            QueryOperation::Select=>{
                let table_name=query.table_name.ok_or(DbError::InsufficientParams)?;
                let conditions=query.conditions.ok_or(DbError::InsufficientParams)?;
                let cols=match query.columns{
                    Some(t)=>t,
                    None=>{
                        let c:Vec<String>=Vec::new();
                        c
                    },
                };
                let rows=self.tables.select(&table_name,conditions,cols)?;
                return Ok(QueryResult::Rows(rows));
            },
            QueryOperation::Delete=>{
                let table_name=query.table_name.ok_or(DbError::InsufficientParams)?;
                let conditions=query.conditions.ok_or(DbError::InsufficientParams)?;
                let c=self.tables.delete(&table_name,conditions)?;
                return Ok(QueryResult::Count(c));
            },
            QueryOperation::GetSchema=>{
                let table_name = query.table_name.ok_or(DbError::InsufficientParams)?;
                let schema = self.tables.get_schema(Some(table_name))?;
                return Ok(QueryResult::Schema(schema));
            }
        };
    }
}
