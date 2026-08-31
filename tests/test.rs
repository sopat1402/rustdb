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

    println!("--- Make table users ---");
    let query = r#"{"task":"create_table","table_name":"users","schema":{"column":"id","type":"UINT32","column":"name","type":"VARCHAR","column":"age","type":"UINT32"}}"#;
    send_query(addr, query).await?;

    println!("--- Insert a row ---");
    let query = r#"{"table_name":"users","task":"insert","row":{"id":"1","name":"Sohum Pathak","age":"20"}}"#;
    send_query(addr,query).await?;

    println!("\n--- Select before update ---");
    let query = r#"{"table_name":"users","task":"select","columns":{},"conditions":{}}"#;
    send_query(addr,query).await?;

    println!("\n--- Update age ---");
    let query = r#"{"table_name":"users","task":"update","conditions":{"column":"id","operator":"e","value":"1"},"updates":{"column":"age","value":"21"}}"#;
    send_query(addr,query).await?;

    println!("\n--- Select after update ---");
    let query = r#"{"table_name":"users","task":"select","columns":{},"conditions":{}}"#;
    send_query(addr,query).await?;

    println!("--- Make table users ---");
    let query = r#"{"task":"create_table","table_name":"users","schema":{"column":"id","type":"UINT32","column":"name","type":"VARCHAR","column":"age","type":"UINT32"}}"#;
    send_query(addr, query).await?;

    println!("--- Delete all records ---");
    let query=r#"{"task":"delete","table_name":"users","conditions":{}}"#;
    send_query(addr,query).await?;

    println!("\n--- Shutdown ---");
    let query = r#"{"task":"shutdown"}"#;
    send_query(addr,query).await?;

    Ok(())
}
