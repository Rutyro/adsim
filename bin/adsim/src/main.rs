#[tokio::main]
async fn main() -> std::io::Result<()> {
    let addr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:48898".to_string());
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    println!("adsim listening on {}", listener.local_addr()?);
    adsim_server::serve(listener).await
}
