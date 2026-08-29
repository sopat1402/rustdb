use rustdb::db_errors::DbError;
use rustdb::tables::{Condition,DataTypes,Tables,Value,Operator};

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
    let mut tables=Tables::bootup().unwrap();
    let t1=String::from("users");
    let t2=String::from("posts");
    let mut s1:Vec<(String,DataTypes)>=Vec::new();
    let mut s2:Vec<(String,DataTypes)>=Vec::new();
    let t1_cols:Vec<String>=vec![String::from("ID"),String::from("Name"),String::from("Age")];
    let t2_cols:Vec<String>=vec![String::from("ID"),String::from("Title"),String::from("Poster")];
    s1.push((t1_cols[0].clone(),DataTypes::UINT32));
    s1.push((t1_cols[1].clone(),DataTypes::VARCHAR));
    s1.push((t1_cols[2].clone(),DataTypes::UINT32));

    s2.push((t2_cols[0].clone(),DataTypes::UINT32));
    s2.push((t2_cols[1].clone(),DataTypes::VARCHAR));
    s2.push((t2_cols[2].clone(),DataTypes::UINT32));

    match tables.create_table(t1.clone(),s1){
        Ok(_)=>{},
        Err(e)=>panic!("{:?}",e),
    };
    match tables.create_table(t2.clone(),s2){
        Ok(_)=>{},
        Err(e)=>panic!("{:?}",e),
    };
    let row:Vec<(String,Value)>=vec![
        (String::from("ID"),Value::Uint32(1)),
        (String::from("Name"),Value::Varchar(String::from("Sohum"))),
        (String::from("Age"),Value::Uint32(20))
    ];
    match tables.insert(&t1,row){
        Ok(_)=>{},
        Err(e)=>panic!("{:?}",e),
    };
    let row:Vec<(String,Value)>=vec![
        (String::from("ID"),Value::Uint32(1)),
        (String::from("Title"),Value::Varchar(String::from("My Balls Itch"))),
        (String::from("Poster"),Value::Uint32(1))
    ];
    match tables.insert(&t2,row){
        Ok(_)=>{},
        Err(e)=>panic!("{:?}",e),
    };

    let res=match tables.select(&t1,vec![],t1_cols.clone()){
        Ok(v)=>v,
        Err(e)=>panic!("{:?}",e),
    };
    println!("Rows of {t1}");
    display_rows(&res);
    println!();
    let res=match tables.select(&t2,vec![],t2_cols.clone()){
        Ok(v)=>v,
        Err(e)=>panic!("{:?}",e),
    };
    println!("Rows of {t2}");
    display_rows(&res);
    println!("\nTest passed\n");
    tables.shutdown().unwrap();
}
