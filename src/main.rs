use clap::Parser;
use tokio::io::{AsyncReadExt,AsyncWriteExt};
use rustdb::db_errors::DbError;
use rustdb::database::{Database,DatabaseHandle};
use rustdb::parser::{encode_query_result,encode_error};

#[derive(Parser)]
struct Args {
    db_name: String,
    #[arg(default_value_t = 5432)]
    port: u16,
    #[arg(long,default_value_t=false)]
    new: bool,
}

#[tokio::main]
async fn main() -> Result<(), DbError> {
    let args = Args::parse();
    let mut db;
    let handle;
    if !args.new{
        (db, handle) = Database::bootup(args.db_name)?;
    }else{
        (db,handle) = Database::new(args.db_name)?;
    }
    tokio::spawn(async move {
        db.run().await;
    });
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", args.port)).await
        .map_err(|_| DbError::FileError)?;
    println!("Started server on port {}",args.port);
    loop {
        let (socket, _addr) = match listener.accept().await{
            Ok(pair)=>pair,
            Err(e)=>{
                eprintln!("Accept failed : {e}");
                continue;
            },
        };
        let handle = handle.clone();

        tokio::spawn(async move {
            handle_connection(socket, handle).await;
        });
    }
}

async fn handle_connection(mut socket: tokio::net::TcpStream, handle: DatabaseHandle) {
    let mut flag_buf = [0u8; 1];
    if socket.read_exact(&mut flag_buf).await.is_err() {
        return;
    }
    let is_shutdown_flag = flag_buf[0] == 1;
    let mut len_buf = [0u8; 4];
    if socket.read_exact(&mut len_buf).await.is_err() {
        return;
    }
    let len = u32::from_be_bytes(len_buf) as usize;
    if is_shutdown_flag && len == 0 {
        let query = String::from("{\"task\":\"shutdown\"}");
        match handle.submit_job(query).await {
            Ok(_) => { let _ = write_response(&mut socket, Ok(b"shutting down")).await; }
            Err(e) => { let _ = write_response(&mut socket, Err(&format!("{e:?}"))).await; }
        }
        return;
    }
    let mut query_buf = vec![0u8; len];
    if socket.read_exact(&mut query_buf).await.is_err() {
        return;
    }
    let query_str = match String::from_utf8(query_buf) {
        Ok(s) => s,
        Err(_) => {
            let _ = write_response(&mut socket, Err("invalid utf-8 in query")).await;
            return;
        }
    };
    match handle.submit_job(query_str).await {
        Ok(result) => {
            let body = encode_query_result(&result);
            let _ = write_response(&mut socket, Ok(body.as_bytes())).await;
        }
        Err(e) => {
            let body=encode_error(&e);
            let _ = write_response(&mut socket, Err(&body)).await;
        }
    }
}

async fn write_response(socket: &mut tokio::net::TcpStream, result: Result<&[u8], &str>) -> std::io::Result<()> {
    let (status, body): (u8, &[u8]) = match result {
        Ok(b) => (0, b),
        Err(msg) => (1, msg.as_bytes()),
    };
    socket.write_all(&[status]).await?;
    socket.write_all(&(body.len() as u32).to_be_bytes()).await?;
    socket.write_all(body).await?;
    Ok(())
}


