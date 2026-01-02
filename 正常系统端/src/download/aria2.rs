use anyhow::Result;
use aria2_ws::response::TaskStatus;
use std::process::Child;
use std::sync::Arc;

use crate::utils::cmd::create_command;
use crate::utils::path::get_bin_dir;

/// 下载进度信息
#[derive(Debug, Clone)]
pub struct DownloadProgress {
    pub gid: String,
    pub completed_length: u64,
    pub total_length: u64,
    pub download_speed: u64,
    pub percentage: f64,
    pub status: DownloadStatus,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DownloadStatus {
    Waiting,
    Active,
    Paused,
    Complete,
    Error(String),
}

/// aria2 下载管理器
pub struct Aria2Manager {
    client: Option<Arc<aria2_ws::Client>>,
    aria2_process: Option<Child>,
}

impl Aria2Manager {
    /// 启动 aria2c 进程并连接
    pub async fn start() -> Result<Self> {
        let bin_dir = get_bin_dir();
        let aria2c_path = bin_dir.join("aria2c.exe");

        if !aria2c_path.exists() {
            anyhow::bail!("aria2c.exe not found at {:?}", aria2c_path);
        }

        // 启动 aria2c 进程，启用 RPC
        let process = create_command(&aria2c_path)
            .args([
                "--enable-rpc=true",
                "--rpc-listen-port=6800",
                "--rpc-allow-origin-all=true",
                "--max-concurrent-downloads=5",
                "--split=16",
                "--max-connection-per-server=16",
                "--min-split-size=1M",
                "--file-allocation=none",
                "--continue=true",
                "--auto-file-renaming=false",
                "--allow-overwrite=true",
            ])
            .spawn()?;

        // 等待 aria2c 启动
        tokio::time::sleep(std::time::Duration::from_millis(800)).await;

        // 连接到 aria2c WebSocket
        let client = aria2_ws::Client::connect("ws://127.0.0.1:6800/jsonrpc", None).await?;

        Ok(Self {
            client: Some(Arc::new(client)),
            aria2_process: Some(process),
        })
    }

    /// 添加下载任务
    pub async fn add_download(
        &self,
        url: &str,
        save_dir: &str,
        filename: Option<&str>,
    ) -> Result<String> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("aria2 client not connected"))?;

        let mut options = aria2_ws::TaskOptions::default();
        options.dir = Some(save_dir.to_string());
        options.split = Some(16);
        options.max_connection_per_server = Some(16);

        if let Some(name) = filename {
            options.out = Some(name.to_string());
        }

        let gid = client
            .add_uri(vec![url.to_string()], Some(options), None, None)
            .await?;

        Ok(gid)
    }

    /// 获取下载状态
    pub async fn get_status(&self, gid: &str) -> Result<DownloadProgress> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("aria2 client not connected"))?;

        let status = client.tell_status(gid).await?;

        let completed = status.completed_length;
        let total = status.total_length;
        let speed = status.download_speed;

        let percentage = if total > 0 {
            (completed as f64 / total as f64) * 100.0
        } else {
            0.0
        };

        let download_status = match status.status {
            TaskStatus::Waiting => DownloadStatus::Waiting,
            TaskStatus::Active => DownloadStatus::Active,
            TaskStatus::Paused => DownloadStatus::Paused,
            TaskStatus::Complete => DownloadStatus::Complete,
            TaskStatus::Error => DownloadStatus::Error(status.error_message.unwrap_or_default()),
            TaskStatus::Removed => DownloadStatus::Error("已移除".to_string()),
        };

        Ok(DownloadProgress {
            gid: gid.to_string(),
            completed_length: completed,
            total_length: total,
            download_speed: speed,
            percentage,
            status: download_status,
        })
    }

    /// 暂停下载
    pub async fn pause(&self, gid: &str) -> Result<()> {
        if let Some(client) = &self.client {
            client.pause(gid).await?;
        }
        Ok(())
    }

    /// 恢复下载
    pub async fn resume(&self, gid: &str) -> Result<()> {
        if let Some(client) = &self.client {
            client.unpause(gid).await?;
        }
        Ok(())
    }

    /// 取消下载
    pub async fn cancel(&self, gid: &str) -> Result<()> {
        if let Some(client) = &self.client {
            client.remove(gid).await?;
        }
        Ok(())
    }

    /// 获取全局状态
    pub async fn get_global_stat(&self) -> Result<(u64, u64)> {
        if let Some(client) = &self.client {
            let stat = client.get_global_stat().await?;
            return Ok((stat.download_speed, stat.num_active as u64));
        }
        Ok((0, 0))
    }

    /// 关闭 aria2c
    pub async fn shutdown(&mut self) -> Result<()> {
        if let Some(client) = self.client.take() {
            let _ = client.shutdown().await;
        }
        if let Some(mut process) = self.aria2_process.take() {
            let _ = process.kill();
        }
        Ok(())
    }
}

impl Drop for Aria2Manager {
    fn drop(&mut self) {
        if let Some(mut process) = self.aria2_process.take() {
            let _ = process.kill();
        }
    }
}
