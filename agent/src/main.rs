mod config;
mod connection;
mod exec;
mod file;
mod forward;
mod monitor;
mod telemetry;

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    let config = config::Config::load()?;
    telemetry::init(&config.log_level);

    tracing::info!(
        agent_id = %config.agent_id,
        conn_mode = %config.conn_mode,
        "helm-agent starting"
    );

    if config.conn_mode == "forward" {
        forward::serve(&config).await
    } else {
        connection::run_agent(&config).await
    }
}
