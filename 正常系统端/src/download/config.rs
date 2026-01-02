use anyhow::Result;
use serde::{Deserialize, Serialize};

/// 在线系统镜像信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnlineSystem {
    pub download_url: String,
    pub display_name: String,
    pub is_win11: bool,
}

/// 在线 PE 信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnlinePE {
    pub download_url: String,
    pub display_name: String,
    pub filename: String,
}

/// 配置管理器
#[derive(Debug, Clone, Default)]
pub struct ConfigManager {
    pub systems: Vec<OnlineSystem>,
    pub pe_list: Vec<OnlinePE>,
}

impl ConfigManager {
    /// 从远程服务器加载配置
    pub async fn load_from_remote(system_url: &str, pe_url: &str) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()?;

        // 下载系统列表
        let systems = if let Ok(resp) = client.get(system_url).send().await {
            if let Ok(text) = resp.text().await {
                Self::parse_system_list(&text)
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        // 下载 PE 列表
        let pe_list = if let Ok(resp) = client.get(pe_url).send().await {
            if let Ok(text) = resp.text().await {
                Self::parse_pe_list(&text)
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        Ok(Self { systems, pe_list })
    }

    /// 从本地文件加载配置
    pub fn load_from_local(exe_dir: &std::path::Path) -> Result<Self> {
        let dl_path = exe_dir.join("dl.txt");
        let pe_path = exe_dir.join("pe.txt");

        let systems = if dl_path.exists() {
            let content = std::fs::read_to_string(&dl_path)?;
            Self::parse_system_list(&content)
        } else {
            Vec::new()
        };

        let pe_list = if pe_path.exists() {
            let content = std::fs::read_to_string(&pe_path)?;
            Self::parse_pe_list(&content)
        } else {
            Vec::new()
        };

        Ok(Self { systems, pe_list })
    }

    /// 解析系统列表
    /// 格式: URL,显示名称,Win11/Win10
    fn parse_system_list(content: &str) -> Vec<OnlineSystem> {
        content
            .lines()
            .filter(|line| !line.trim().is_empty() && !line.trim().starts_with('#'))
            .filter_map(|line| {
                let parts: Vec<&str> = line.split(',').collect();
                if parts.len() >= 3 {
                    Some(OnlineSystem {
                        download_url: parts[0].trim().to_string(),
                        display_name: parts[1].trim().to_string(),
                        is_win11: parts[2].trim().eq_ignore_ascii_case("Win11"),
                    })
                } else if parts.len() >= 2 {
                    Some(OnlineSystem {
                        download_url: parts[0].trim().to_string(),
                        display_name: parts[1].trim().to_string(),
                        is_win11: parts[1].to_lowercase().contains("11"),
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    /// 解析 PE 列表
    /// 格式: URL,显示名称,文件名
    fn parse_pe_list(content: &str) -> Vec<OnlinePE> {
        content
            .lines()
            .filter(|line| !line.trim().is_empty() && !line.trim().starts_with('#'))
            .filter_map(|line| {
                let parts: Vec<&str> = line.split(',').collect();
                if parts.len() >= 3 {
                    Some(OnlinePE {
                        download_url: parts[0].trim().to_string(),
                        display_name: parts[1].trim().to_string(),
                        filename: parts[2].trim().to_string(),
                    })
                } else if parts.len() >= 2 {
                    let url = parts[0].trim();
                    let filename = url.split('/').last().unwrap_or("pe.wim").to_string();
                    Some(OnlinePE {
                        download_url: url.to_string(),
                        display_name: parts[1].trim().to_string(),
                        filename,
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    /// 保存配置到本地
    pub fn save_to_local(&self, exe_dir: &std::path::Path) -> Result<()> {
        // 保存系统列表
        let dl_content: String = self
            .systems
            .iter()
            .map(|s| {
                format!(
                    "{},{},{}",
                    s.download_url,
                    s.display_name,
                    if s.is_win11 { "Win11" } else { "Win10" }
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(exe_dir.join("dl.txt"), dl_content)?;

        // 保存 PE 列表
        let pe_content: String = self
            .pe_list
            .iter()
            .map(|p| format!("{},{},{}", p.download_url, p.display_name, p.filename))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(exe_dir.join("pe.txt"), pe_content)?;

        Ok(())
    }

    /// 检查配置是否为空
    pub fn is_empty(&self) -> bool {
        self.systems.is_empty() && self.pe_list.is_empty()
    }
}
