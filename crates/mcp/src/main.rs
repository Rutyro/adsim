use adsim_client::AdsClient;
use adsim_mcp::{AdsimMcp, Config};
use rmcp::ServiceExt;
use rmcp::transport::stdio;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = match Config::parse(std::env::args().skip(1)) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("{error}");
            eprintln!("usage: adsim-mcp [--ads HOST:PORT] [--real AMS.NET.ID] [--write SYMBOL]...");
            std::process::exit(2);
        }
    };

    let client = AdsClient::connect(&config.ads, config.target_net_id()).await?;
    let server = AdsimMcp::new(client, config).serve(stdio()).await?;
    server.waiting().await?;
    Ok(())
}
