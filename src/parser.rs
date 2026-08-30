use crate::tables::{Condition, Value, Operator,DataTypes};
use crate::database::QueryResult;
use std::collections::HashMap;
use crate::db_errors::DbError;

enum Token{
    Key(String),
    Value(String),
}

pub enum QueryOperation{
    Insert,
    Select,
    Delete,
    Update,
    CreateTable,
    DeleteTable,
    DropDB,
    CreateDB,
    Shutdown,
    GetSchema,
}

pub struct Query{
    pub table_name  :   Option<String>,
    pub db_name     :   Option<String>,
    pub task        :   QueryOperation,
    pub row         :   Option<Vec<(String,Value)>>,
    pub columns     :   Option<Vec<String>>,
    pub conditions  :   Option<Vec<Condition>>,
    pub updates     :   Option<Vec<Condition>>,
    pub schema      :   Option<Vec<(String,DataTypes)>>,
}

fn parse_data_type(raw:&str)->Result<DataTypes,DbError>{
    match raw.to_uppercase().as_str(){
        "UINT32"=>Ok(DataTypes::UINT32),
        "INT32"=>Ok(DataTypes::INT32),
        "FLOAT32"=>Ok(DataTypes::FLOAT32),
        "VARCHAR"=>Ok(DataTypes::VARCHAR),
        _=>Err(DbError::TypeMismatch),
    }
}

fn lexer(query:String)->Result<Vec<Token>,DbError>{
    let mut in_string:bool=false;
    let mut buf=String::from("");
    let mut is_escape=false;
    let mut is_value=false;
    let mut tokens:Vec<Token>=Vec::new();
    for c in query.chars(){
        if c=='"'{
            if is_escape{
                buf.push('"');
                is_escape=false;
            }else{
                in_string=!in_string;
                if buf.len()!=0{
                    if is_value{
                        tokens.push(Token::Value(buf.clone()));
                    }else{
                        tokens.push(Token::Key(buf.clone()));
                    }
                }
                is_escape=false;
                buf.clear();
            }
        }
        else if c=='\\'{
            if is_escape{
                buf.push('\\');
                is_escape=false;
            }else{
                is_escape=true;
            }
        }
        else if c==':' && !in_string{
            is_value=true;
            continue;
        }
        else if (c=='}'||c==',') && !in_string{
            is_value=false;
            continue;
        }
        else if c=='{' && !in_string{
            is_value=false;
            continue;
        }
        else{
            buf.push(c);
        }
    }
    Ok(tokens)
}

fn next_value(tokens:&[Token], i:&mut usize)->Result<String,DbError>{
    *i+=1;
    match tokens.get(*i){
        None | Some(Token::Key(_)) => Err(DbError::MalformedRequest),
        Some(Token::Value(v)) => {
            let v=v.clone();
            *i+=1;
            Ok(v)
        }
    }
}

fn parse_operation(op:&str)->Result<Operator,DbError>{
    match op.to_lowercase().as_str(){
        "ge"=>Ok(Operator::GreaterEqual),
        "le"=>Ok(Operator::LessEqual),
        "e"=>Ok(Operator::Equal),
        "ne"=>Ok(Operator::NotEqual),
        "l"=>Ok(Operator::Less),
        "g"=>Ok(Operator::Greater),
        _=>Err(DbError::InvalidOperation),
    }
}

fn read_field_group(tokens:&[Token],i:&mut usize,expected_keys:&[&str],)->Result<HashMap<String,String>,DbError>{
    let mut fields=HashMap::new();
    while fields.len()<expected_keys.len() && *i<tokens.len(){
        match &tokens[*i]{
            Token::Key(k) if expected_keys.contains(&k.as_str()) => {
                let key=k.clone();
                let value=next_value(tokens,i)?;
                fields.insert(key,value);
            }
            _ => break,
        }
    }
    Ok(fields)
}

fn escape_json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out
}

fn encode_value(value: &Value) -> String {
    match value {
        Value::Uint32(v) => v.to_string(),
        Value::Int32(v) => v.to_string(),
        Value::Float32(v) => v.to_string(),
        Value::Varchar(v) => format!("\"{}\"", escape_json_string(v)),
    }
}

pub fn encode_error(err: &DbError) -> String {
    format!(
        "{{\"success\":false,\"error\":\"{}\"}}",
        escape_json_string(&format!("{err:?}"))
    )
}

pub fn encode_query_result(result: &QueryResult) -> String {
    match result {
        QueryResult::Success => {
            "{\"success\":true,\"type\":\"success\"}".to_string()
        }
        QueryResult::Killed => {
            "{\"success\":true,\"type\":\"success\"}".to_string()
        }
        QueryResult::Count(c) => {
            format!("{{\"success\":true,\"type\":\"count\",\"count\":{c}}}")
        }
        QueryResult::Rows(rows) => {
            let rows_json: Vec<String> = rows.iter().map(|row| {
                let fields: Vec<String> = row.iter().map(|(col, val)| {
                    format!(
                        "{{\"column\":\"{}\",\"value\":{}}}",
                        escape_json_string(col),
                        encode_value(val)
                    )
                }).collect();
                format!("[{}]", fields.join(","))
            }).collect();
            format!(
                "{{\"success\":true,\"type\":\"rows\",\"rows\":[{}]}}",
                rows_json.join(",")
            )
        }
        _=>return String::from("you peeked already"),
    }
}

pub fn peek_table_name(query: &str) -> Result<Option<String>, DbError>{
    let tokens = lexer(query.to_string())?;
    for i in 0..tokens.len(){
        if let Token::Key(k) = &tokens[i]{
            if k == "table_name"{
                if let Some(Token::Value(v)) = tokens.get(i+1){
                    return Ok(Some(v.clone()));
                }
            }
        }
    }
    Ok(None)
}

pub fn parse(query:String,schema:Option<&Vec<(String,DataTypes)>>)->Result<Query,DbError>{
    let tokens=lexer(query)?;
    let mut table_name:Option<String>=None;
    let mut task:Option<QueryOperation>=None;
    let mut conditions:Option<Vec<Condition>>=None;
    let mut updates:Option<Vec<Condition>>=None;
    let mut row:Option<Vec<(String,Value)>>=None;
    let mut columns:Option<Vec<String>>=None;
    let mut i:usize=0;
    let mut db_name:Option<String>=None;
    let mut schema_ret:Option<Vec<(String,DataTypes)>>=None;

    while i<tokens.len(){
        match &tokens[i]{
            Token::Key(key) => {
                match key.as_str(){
                    "table_name" => {
                        table_name=Some(next_value(&tokens,&mut i)?);
                    }
                    "db_name"=>{
                        db_name=Some(next_value(&tokens,&mut i)?);
                    }
                    "task" => {
                        let v=next_value(&tokens,&mut i)?.to_lowercase();
                        task=Some(match v.as_str(){
                            "insert"=>QueryOperation::Insert,
                            "delete"=>QueryOperation::Delete,
                            "update"=>QueryOperation::Update,
                            "select"=>QueryOperation::Select,
                            "drop_db"=>QueryOperation::DropDB,
                            "delete_table"=>QueryOperation::DeleteTable,
                            "create_table"=>QueryOperation::CreateTable,
                            "create_db"=>QueryOperation::CreateDB,
                            "shutdown"=>QueryOperation::Shutdown,
                            _=>return Err(DbError::InvalidOperation),
                        });
                    }
                    "row" => {
                        let mut row_vals:Vec<(String,Value)>=Vec::new();
                        i+=1;
                        let schema = schema.ok_or(DbError::InsufficientParams)?;
                        while i<tokens.len(){
                            match &tokens[i]{
                                Token::Key(col) => {
                                    let col=col.clone();
                                    let raw=next_value(&tokens,&mut i)?;
                                    let dtype=lookup_type(&schema,&col)?;
                                    let val=coerce_value(&raw,dtype)?;
                                    row_vals.push((col,val));
                                }
                                Token::Value(_) => break,
                            }
                        }
                        row=Some(row_vals);
                    }
                    "conditions" => {
                        i+=1;
                        let schema = schema.ok_or(DbError::InsufficientParams)?;
                        let fields=read_field_group(&tokens,&mut i,&["column","operator","value"])?;
                        if fields.is_empty(){
                            conditions=Some(Vec::new());
                        }else{
                            conditions=Some(vec![build_condition(fields,&schema)?]);
                        }
                    }
                    "updates" => {
                        i+=1;
                        let schema = schema.ok_or(DbError::InsufficientParams)?;
                        let fields=read_field_group(&tokens,&mut i,&["column","value"])?;
                        updates=Some(vec![build_condition(fields,&schema)?]);
                    }
                    "columns" => {
                        let mut col_vals:Vec<String>=Vec::new();
                        i+=1;
                        while i<tokens.len(){
                            match &tokens[i]{
                                Token::Key(k) if k=="column" => {
                                    let val=next_value(&tokens,&mut i)?;
                                    col_vals.push(val);
                                }
                                _ => break,
                            }
                        }
                        columns=Some(col_vals);
                    }
                    "schema" => {
                        let mut schema_vals:Vec<(String,DataTypes)>=Vec::new();
                        i+=1;
                        loop{
                            let fields=read_field_group(&tokens,&mut i,&["column","type"])?;
                            if fields.is_empty(){
                                break;
                            }
                            let col=fields.get("column").ok_or(DbError::MalformedRequest)?.clone();
                            let raw_type=fields.get("type").ok_or(DbError::MalformedRequest)?;
                            let dtype=parse_data_type(raw_type)?;
                            schema_vals.push((col,dtype));
                        }
                        schema_ret=Some(schema_vals);
                    }
                    _ => {
                        i+=1;
                        if i<tokens.len(){
                            if let Token::Value(_)=&tokens[i]{
                                i+=1;
                            }
                        }
                    }
                }
            }
            Token::Value(_) => return Err(DbError::MalformedRequest),
        }
    }

    Ok(Query{
        table_name,
        db_name,
        task: task.ok_or(DbError::MalformedRequest)?,
        row,
        columns,
        conditions,
        updates,
        schema:schema_ret,
    })
}

fn coerce_value(raw:&str, dtype:&DataTypes)->Result<Value,DbError>{
    match dtype{
        DataTypes::UINT32 => raw.parse::<u32>()
            .map(Value::Uint32)
            .map_err(|_| DbError::TypeMismatch),
        DataTypes::INT32 => raw.parse::<i32>()
            .map(Value::Int32)
            .map_err(|_| DbError::TypeMismatch),
        DataTypes::FLOAT32 => raw.parse::<f32>()
            .map(Value::Float32)
            .map_err(|_| DbError::TypeMismatch),
        DataTypes::VARCHAR => Ok(Value::Varchar(raw.to_string())),
    }
}

fn lookup_type<'a>(schema:&'a [(String,DataTypes)], column:&str)->Result<&'a DataTypes,DbError>{
    schema.iter()
        .find(|(name,_)| name==column)
        .map(|(_,dtype)| dtype)
        .ok_or(DbError::InvalidColumn)
}

fn build_condition(fields:std::collections::HashMap<String,String>,schema:&[(String,DataTypes)],)->Result<Condition,DbError>{
    let column=fields.get("column").ok_or(DbError::MalformedRequest)?.clone();
    let raw_value=fields.get("value").ok_or(DbError::MalformedRequest)?;
    let dtype=lookup_type(schema,&column)?;
    let value=coerce_value(raw_value,dtype)?;
    let operator=match fields.get("operator"){
        Some(op)=>parse_operation(op)?,
        None=>Operator::Equal,
    };
    Ok(Condition{ column, value, operator })
}
