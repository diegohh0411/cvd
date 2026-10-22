//! Stdio MCP server over the app's database. stdout carries the protocol;
//! diagnostics go to stderr.

mod mcp;

use cvd_core::store::Store;
use mcp::{db_path_from, CvServer};
use rmcp::ServiceExt;

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("cvd-mcp: {e}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let path = db_path_from(std::env::var_os("CVD_DB_PATH").map(Into::into))
        .ok_or("cannot resolve data dir; set CVD_DB_PATH")?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    eprintln!("cvd-mcp: using {}", path.display());
    let store = Store::open_external(&format!("sqlite://{}", path.display())).await?;
    CvServer::new(store)
        .serve(rmcp::transport::stdio())
        .await?
        .waiting()
        .await?;
    Ok(())
}
