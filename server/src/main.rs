use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    helm_server::run().await
}
