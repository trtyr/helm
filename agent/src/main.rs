// Windows 上以 GUI 子系统运行（去黑窗口），日志落文件；其他平台保持控制台。
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod cert;
mod config;
mod connection;
mod exec;
mod file;
mod forward;
mod fs;
mod monitor;
mod process;
mod pty;
mod service;
mod telemetry;
mod uninstall;

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    let config = config::Config::load()?;
    telemetry::init(&config.log_level, &config.log_dir);

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
