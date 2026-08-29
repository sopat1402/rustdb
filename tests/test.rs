use rustdb::index::Index;
use rustdb::page::DatabaseFile;
use rustdb::db_errors::DbError;
use std::fs::File;
use rustdb::tables::{Condition,DataTypes,Table,Value,Operator};

fn bootup()->Result<Index,DbError>{
    let file=File::options()
        .read(true)
        .create(true)
        .write(true)
        .open("database.db").map_err(|_| DbError::FileError)?;
    let size=file.metadata().map_err(|_| DbError::FileError)?.len();
    let btree=File::options()
        .read(true)
        .write(true)
        .create(true)
        .open("btree.tree").map_err(|_| DbError::FileError)?;
    let page_metadata=File::options()
        .read(true)
        .write(true)
        .create(true)
        .open("page.meta").map_err(|_| DbError::FileError)?;
    let db_file=DatabaseFile{
        file,
        page_metadata,
        btree,
        size,
    };
    let mut index=Index::new(db_file)?;
    if index.wal.length!=0{
        index.recover()?;
    }
    Ok(index)
}

fn display_rows(res:&Vec<Vec<(String,Value)>>){
    for row in res{
        for (col,value) in row{
            print!("{col} : ");
            match value{
                Value::Uint32(v)=>print!("{v}, "),
                Value::Int32(v)=>print!("{v}, "),
                Value::Float32(v)=>print!("{v}, "),
                Value::Varchar(v)=>print!("{v}, "),
            };
        }
        print!("\n");
    }
}

#[test]
fn crud_test(){
    //init
    println!("Intitialising and bootup...\n");
    let table_name=String::from("crud_test_table");
    let mut index=bootup().unwrap();
    let mut schema:Vec<(String,DataTypes)>=Vec::new();
    let col_id=String::from("id");
    let col_age=String::from("age");
    let col_name=String::from("name");
    schema.push((col_id.clone(),DataTypes::UINT32));
    schema.push((col_age.clone(),DataTypes::UINT32));
    schema.push((col_name.clone(),DataTypes::VARCHAR));
    let mut table=Table::new(&table_name,schema,1).unwrap();

    //pushing data
    println!("Pushing 3 rows\n");
    for (id,age,name) in [(1u32,30u32,"Alice"),(2u32,25u32,"Bob"),(3u32,40u32,"Carol")]{
        let mut row:Vec<(String,Value)>=Vec::new();
        row.push((col_id.clone(),Value::Uint32(id)));
        row.push((col_age.clone(),Value::Uint32(age)));
        row.push((col_name.clone(),Value::Varchar(name.to_string())));
        table.insert(&mut index,row).unwrap();
    }

    //getting all rows and all columns
    let all_cols=vec![col_id.clone(),col_age.clone(),col_name.clone()];
    println!("Getting all rows\n");
    let res=table.select(&mut index,Vec::new(),all_cols.clone()).unwrap();
    assert_eq!(res.len(),3);
    display_rows(&res);
    println!();

    //selecting specific ones
    println!("Getting rows where age > 25\n");
    let conditions=vec![Condition{
        column: col_age.clone(),
        operator: Operator::Greater,
        value: Value::Uint32(25),
    }];
    let res=table.select(&mut index,conditions,all_cols.clone()).unwrap();
    match res.len(){
        2=>{},
        _=>println!("Wrong number of rows retrieved  : {}",res.len()),
    };
    display_rows(&res);
    assert_eq!(res.len(),2);
    println!();

    //selecting on name and updating
    println!("Updating row where name is Bob to have age 99");
    let conditions=vec![Condition{
        column: col_name.clone(),
        operator: Operator::Equal,
        value: Value::Varchar("Bob".to_string()),
    }];
    let updates=vec![Condition{
        column: col_age.clone(),
        operator: Operator::Equal,
        value: Value::Uint32(99),
    }];
    let updated_count=table.update(&mut index,conditions,updates).unwrap();
    assert_eq!(updated_count,1);
    println!("Updated\n");

    //confirming update
    println!("Confirming update\n");
    let conditions=vec![Condition{
        column: col_name.clone(),
        operator: Operator::Equal,
        value: Value::Varchar("Bob".to_string()),
    }];
    let res=table.select(&mut index,conditions,all_cols.clone()).unwrap();
    assert_eq!(res.len(),1);
    display_rows(&res);
    println!("\nUpdated successfully \n");

    //deleting id 1
    println!("Deleting ID 1");
    let conditions=vec![Condition{
        column: col_id.clone(),
        operator: Operator::Equal,
        value: Value::Uint32(1),
    }];
    let deleted_count=table.delete(&mut index,conditions.clone()).unwrap();
    assert_eq!(deleted_count,1);
    println!("Deleted\n");

    //confirming deletion
    let res=table.select(&mut index,Vec::new(),all_cols.clone()).unwrap();
    display_rows(&res);
    assert_eq!(res.len(),2);
    println!("Deletion confirmed\n");

    //testing what happens when I take the deleted record
    println!("Testing retrieval of deleted record\n");
    let res=table.select(&mut index,conditions,all_cols.clone()).unwrap();
    if res.len()==0{
        println!("No rows in result, successful deletion");
    }
    assert_eq!(res.len(),0);
    println!("CRUD test passed!\n");


    println!("Shutting down");
    table.serialise().unwrap();
    index.shutdown().unwrap();
    let mut index=bootup().unwrap();
    let table=Table::deserialise(table_name.clone()).unwrap();
    let res=table.select(&mut index,Vec::new(),all_cols.clone()).unwrap();
    assert_eq!(res.len(),2);
    index.shutdown().unwrap();
}
