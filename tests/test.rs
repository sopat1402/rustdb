use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

async fn send_query(addr: &str, query: &str) -> std::io::Result<()> {
    let mut socket = TcpStream::connect(addr).await?;
    let query_bytes = query.as_bytes();
    socket.write_all(&[0u8]).await?;
    socket.write_all(&(query_bytes.len() as u32).to_be_bytes()).await?;
    socket.write_all(query_bytes).await?;
    let mut status = [0u8; 1];
    socket.read_exact(&mut status).await?;
    let mut len_buf = [0u8; 4];
    socket.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    let mut body = vec![0u8; len];
    socket.read_exact(&mut body).await?;
    println!("status: {}", status[0]);
    println!("body: {}", String::from_utf8_lossy(&body));
    Ok(())
}

#[tokio::main]
#[test]
async fn test() -> std::io::Result<()> {

    let addr = "127.0.0.1:5432";

    println!("--- Select all ID and name ---");
    let query = r#"{"table_name":"users","task":"select","columns":{"column":"id","column":"name"},"conditions":{}}"#;
    send_query(addr,query).await?;

    println!("\n--- Select all columns ---");
    let query = r#"{"table_name":"users","task":"select","columns":{},"conditions":{}}"#;
    send_query(addr,query).await?;

    println!("\n--- Select all columns ---");
    let query = r#"{"table_name":"users","task":"select","columns":{},"conditions":{}}"#;
    send_query(addr,query).await?;

    println!("\n--- Shutdown ---");
    let query=r#"{"task":"shutdown"}"#;
    send_query(addr,query).await?;

    Ok(())
}
