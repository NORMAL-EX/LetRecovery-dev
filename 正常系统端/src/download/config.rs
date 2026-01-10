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

/// 在线软件信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnlineSoftware {
    /// 软件名称
    pub name: String,
    /// 软件描述
    pub description: String,
    /// 更新日期
    pub update_date: String,
    /// 文件大小
    pub file_size: String,
    /// 图标URL（可选）
    #[serde(default)]
    pub icon_url: Option<String>,
    /// 下载URL（64位）
    pub download_url: String,
    /// 下载URL（32位，可选）
    #[serde(default)]
    pub download_url_x86: Option<String>,
    /// XP系统下载URL（可选）
    #[serde(default)]
    pub download_url_nt5: Option<String>,
    /// 文件名
    pub filename: String,
}

/// 软件列表JSON格式
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoftwareList {
    pub software: Vec<OnlineSoftware>,
}

/// 配置管理器
#[derive(Debug, Clone, Default)]
pub struct ConfigManager {
    pub systems: Vec<OnlineSystem>,
    pub pe_list: Vec<OnlinePE>,
    pub software_list: Vec<OnlineSoftware>,
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

        Ok(Self { systems, pe_list, software_list: Vec::new() })
    }
    
    /// 从远程配置内容加载
    /// 
    /// # Arguments
    /// * `dl_content` - 系统镜像列表内容
    /// * `pe_content` - PE 列表内容
    pub fn load_from_content(dl_content: Option<&str>, pe_content: Option<&str>) -> Self {
        let systems = dl_content
            .map(|c| Self::parse_system_list(c))
            .unwrap_or_default();
        
        let pe_list = pe_content
            .map(|c| Self::parse_pe_list(c))
            .unwrap_or_default();
        
        Self { systems, pe_list, software_list: Vec::new() }
    }
    
    /// 从远程配置内容加载（包含软件列表）
    /// 
    /// # Arguments
    /// * `dl_content` - 系统镜像列表内容
    /// * `pe_content` - PE 列表内容
    /// * `soft_content` - 软件列表内容（JSON格式）
    pub fn load_from_content_with_soft(
        dl_content: Option<&str>, 
        pe_content: Option<&str>,
        soft_content: Option<&str>,
    ) -> Self {
        let systems = dl_content
            .map(|c| Self::parse_system_list(c))
            .unwrap_or_default();
        
        let pe_list = pe_content
            .map(|c| Self::parse_pe_list(c))
            .unwrap_or_default();
        
        let software_list = soft_content
            .map(|c| Self::parse_software_list(c))
            .unwrap_or_default();
        
        Self { systems, pe_list, software_list }
    }

    /// 解析系统列表
    /// 格式: URL,显示名称,Win11/Win10
    pub fn parse_system_list(content: &str) -> Vec<OnlineSystem> {
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
    pub fn parse_pe_list(content: &str) -> Vec<OnlinePE> {
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
    
    /// 解析软件列表（JSON格式）
    pub fn parse_software_list(content: &str) -> Vec<OnlineSoftware> {
        match serde_json::from_str::<SoftwareList>(content) {
            Ok(list) => list.software,
            Err(e) => {
                log::warn!("解析软件列表失败: {}", e);
                Vec::new()
            }
        }
    }

    /// 检查配置是否为空
    pub fn is_empty(&self) -> bool {
        self.systems.is_empty() && self.pe_list.is_empty()
    }
    
    /// 检查软件列表是否为空
    pub fn has_software(&self) -> bool {
        !self.software_list.is_empty()
    }
}
