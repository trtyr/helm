// Windows 上以 GUI 子系统运行（去黑窗口），日志落文件；其他平台保持控制台。
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod cert;
mod child;
mod config;
mod connection;
mod encoding;
mod exec;
mod file;
mod forward;
mod fs;
mod ir;
mod monitor;
mod privilege;
mod process;
mod proxy;
mod pty;
mod service;
mod sys_service;
mod telemetry;
mod uninstall;

#[cfg(windows)]
mod win_native;

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    let config = config::Config::load()?;
    telemetry::init(&config.log_level, &config.log_dir);
    telemetry::spawn_log_retention(&config.log_dir, config.log_keep_days);

    tracing::info!(
        agent_id = %config.agent_id,
        conn_mode = %config.conn_mode,
        "helm-agent starting"
    );

    if config.conn_mode == "forward" {
        let cert = if config.cert_dir.is_empty() {
            None
        } else {
            Some(cert::load_cached(&config.cert_dir)?)
        };
        forward::serve(&config, cert).await
    } else {
        connection::run_agent(&config).await
    }
}
