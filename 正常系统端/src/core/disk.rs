use anyhow::Result;
use std::path::Path;
use crate::utils::cmd::create_command;
use windows::core::PCWSTR;
use windows::Win32::Storage::FileSystem::{
    GetDiskFreeSpaceExW, GetDriveTypeW, GetVolumeInformationW,
};

// DRIVE_FIXED = 3
const DRIVE_FIXED: u32 = 3;

use crate::utils::encoding::gbk_to_utf8;
use crate::utils::path::get_bin_dir;

/// 分区表类型
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum PartitionStyle {
    GPT,
    MBR,
    #[default]
    Unknown,
}

impl std::fmt::Display for PartitionStyle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PartitionStyle::GPT => write!(f, "GPT"),
            PartitionStyle::MBR => write!(f, "MBR"),
            PartitionStyle::Unknown => write!(f, "未知"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Partition {
    pub letter: String,
    pub total_size_mb: u64,
    pub free_size_mb: u64,
    pub label: String,
    pub is_system_partition: bool,
    pub has_windows: bool,
    pub partition_style: PartitionStyle,
    pub disk_number: Option<u32>,
    pub partition_number: Option<u32>,
}

/// 分区详细信息
#[derive(Debug, Clone)]
pub struct PartitionDetail {
    pub style: PartitionStyle,
    pub disk_number: Option<u32>,
    pub partition_number: Option<u32>,
}

pub struct DiskManager;

impl DiskManager {
    /// 获取所有固定磁盘分区列表
    pub fn get_partitions() -> Result<Vec<Partition>> {
        let mut partitions = Vec::new();
        let is_pe = Self::is_pe_environment();

        for letter in b'A'..=b'Z' {
            let drive = format!("{}:", letter as char);
            if let Ok(info) = Self::get_partition_info(&drive, is_pe) {
                partitions.push(info);
            }
        }

        Ok(partitions)
    }

    fn get_partition_info(drive: &str, is_pe: bool) -> Result<Partition> {
        let path = format!("{}\\", drive);
        let wide_path: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();

        // 获取驱动器类型
        let drive_type = unsafe { GetDriveTypeW(PCWSTR(wide_path.as_ptr())) };
        if drive_type != DRIVE_FIXED {
            anyhow::bail!("Not a fixed drive");
        }

        // 获取磁盘空间
        let mut free_bytes_available: u64 = 0;
        let mut total_bytes: u64 = 0;
        let mut total_free_bytes: u64 = 0;

        unsafe {
            GetDiskFreeSpaceExW(
                PCWSTR(wide_path.as_ptr()),
                Some(&mut free_bytes_available as *mut u64),
                Some(&mut total_bytes as *mut u64),
                Some(&mut total_free_bytes as *mut u64),
            )?;
        }

        // 获取卷标
        let mut volume_name = [0u16; 261];
        unsafe {
            let _ = GetVolumeInformationW(
                PCWSTR(wide_path.as_ptr()),
                Some(&mut volume_name),
                None,
                None,
                None,
                None,
            );
        }
        let label = String::from_utf16_lossy(&volume_name)
            .trim_end_matches('\0')
            .to_string();

        // 检查是否为当前系统分区
        let system_drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".to_string());
        let is_current_system = drive.eq_ignore_ascii_case(&system_drive);

        // 检查是否包含 Windows 系统
        let windows_path = format!("{}\\Windows\\System32", drive);
        let has_windows = Path::new(&windows_path).exists();

        // 在 PE 环境下，is_system_partition 表示是否包含 Windows
        // 在正常环境下，is_system_partition 表示是否是当前系统盘
        let is_system_partition = if is_pe {
            has_windows && !is_current_system  // PE下排除 X: 盘
        } else {
            is_current_system
        };

        // 获取分区表类型、磁盘号和分区号
        let detail = Self::get_partition_style(drive);

        Ok(Partition {
            letter: drive.to_string(),
            total_size_mb: total_bytes / 1024 / 1024,
            free_size_mb: free_bytes_available / 1024 / 1024,
            label,
            is_system_partition,
            has_windows,
            partition_style: detail.style,
            disk_number: detail.disk_number,
            partition_number: detail.partition_number,
        })
    }

    /// 获取分区表类型和分区号 (GPT/MBR)
    fn get_partition_style(drive: &str) -> PartitionDetail {
        // 使用 PowerShell 获取分区信息
        let ps_script = format!(
            r#"$partition = Get-Partition -DriveLetter '{}' -ErrorAction SilentlyContinue
if ($partition) {{
    $disk = Get-Disk -Number $partition.DiskNumber -ErrorAction SilentlyContinue
    if ($disk) {{
        Write-Output "$($disk.PartitionStyle)|$($partition.DiskNumber)|$($partition.PartitionNumber)"
    }}
}}"#,
            drive.chars().next().unwrap_or('C')
        );

        let output = create_command("powershell")
            .args(["-NoProfile", "-Command", &ps_script])
            .output();

        if let Ok(output) = output {
            let stdout = gbk_to_utf8(&output.stdout).trim().to_string();
            let parts: Vec<&str> = stdout.split('|').collect();
            
            if parts.len() >= 3 {
                let style = match parts[0].to_uppercase().as_str() {
                    "GPT" => PartitionStyle::GPT,
                    "MBR" => PartitionStyle::MBR,
                    _ => PartitionStyle::Unknown,
                };
                let disk_num = parts[1].parse::<u32>().ok();
                let part_num = parts[2].parse::<u32>().ok();
                return PartitionDetail {
                    style,
                    disk_number: disk_num,
                    partition_number: part_num,
                };
            }
        }

        // 备用方法：使用 diskpart
        Self::get_partition_style_diskpart(drive)
    }

    /// 使用 diskpart 获取分区信息（备用方法）
    fn get_partition_style_diskpart(drive: &str) -> PartitionDetail {
        let letter = drive.chars().next().unwrap_or('C');
        let script = format!("select volume {}\ndetail volume", letter);
        
        let temp_dir = std::env::temp_dir();
        let script_path = temp_dir.join("dp_style.txt");
        
        if std::fs::write(&script_path, &script).is_err() {
            return PartitionDetail {
                style: PartitionStyle::Unknown,
                disk_number: None,
                partition_number: None,
            };
        }

        let output = match create_command("diskpart")
            .args(["/s", script_path.to_str().unwrap()])
            .output()
        {
            Ok(o) => o,
            Err(_) => {
                let _ = std::fs::remove_file(&script_path);
                return PartitionDetail {
                    style: PartitionStyle::Unknown,
                    disk_number: None,
                    partition_number: None,
                };
            }
        };

        let _ = std::fs::remove_file(&script_path);
        let stdout = gbk_to_utf8(&output.stdout);
        
        // 解析磁盘号和分区号
        let mut disk_num: Option<u32> = None;
        let mut part_num: Option<u32> = None;
        
        for line in stdout.lines() {
            let line_upper = line.to_uppercase();
            // 匹配 "磁盘 0" 或 "Disk 0"
            if (line_upper.contains("磁盘") || line_upper.contains("DISK")) 
                && !line_upper.contains("磁盘 ID") && !line_upper.contains("DISK ID") 
            {
                if let Some(num) = line.split_whitespace()
                    .find(|s| s.parse::<u32>().is_ok())
                {
                    disk_num = num.parse().ok();
                }
            }
            // 匹配 "分区 1" 或 "Partition 1"
            if line_upper.contains("分区") || line_upper.contains("PARTITION") {
                if let Some(num) = line.split_whitespace()
                    .find(|s| s.parse::<u32>().is_ok())
                {
                    part_num = num.parse().ok();
                }
            }
        }

        // 如果找到了磁盘号，再查询磁盘的分区表类型
        let style = if let Some(num) = disk_num {
            Self::get_disk_partition_style(num)
        } else {
            PartitionStyle::Unknown
        };

        PartitionDetail {
            style,
            disk_number: disk_num,
            partition_number: part_num,
        }
    }

    /// 获取指定磁盘的分区表类型
    fn get_disk_partition_style(disk_number: u32) -> PartitionStyle {
        let script = format!("select disk {}\ndetail disk", disk_number);
        let temp_dir = std::env::temp_dir();
        let script_path = temp_dir.join("dp_disk_style.txt");
        
        if std::fs::write(&script_path, &script).is_err() {
            return PartitionStyle::Unknown;
        }

        let output = match create_command("diskpart")
            .args(["/s", script_path.to_str().unwrap()])
            .output()
        {
            Ok(o) => o,
            Err(_) => {
                let _ = std::fs::remove_file(&script_path);
                return PartitionStyle::Unknown;
            }
        };

        let _ = std::fs::remove_file(&script_path);
        let stdout = gbk_to_utf8(&output.stdout).to_uppercase();
        
        if stdout.contains("GPT") {
            PartitionStyle::GPT
        } else if stdout.contains("MBR") {
            PartitionStyle::MBR
        } else {
            PartitionStyle::Unknown
        }
    }

    /// 格式化指定分区
    pub fn format_partition(partition: &str) -> Result<String> {
        let bin_dir = get_bin_dir();
        let format_exe = if Self::is_pe_environment() {
            bin_dir.join("format.com").to_string_lossy().to_string()
        } else {
            "format.com".to_string()
        };

        let output = create_command(&format_exe)
            .args([partition, "/FS:NTFS", "/q", "/y"])
            .output()?;

        Ok(gbk_to_utf8(&output.stdout))
    }

    /// 从指定分区缩小并创建新分区
    pub fn shrink_and_create_partition(
        source_partition: &str,
        new_letter: &str,
        size_mb: u64,
    ) -> Result<String> {
        let script_content = format!(
            "select volume {}\nshrink desired={}\ncreate partition primary size={}\nformat fs=ntfs quick\nassign letter={}",
            source_partition.chars().next().unwrap_or('C'),
            size_mb,
            size_mb,
            new_letter.chars().next().unwrap_or('Y').to_ascii_lowercase()
        );

        let temp_dir = std::env::temp_dir();
        let script_path = temp_dir.join("dp_script.txt");
        std::fs::write(&script_path, &script_content)?;

        let output = create_command("diskpart")
            .args(["/s", script_path.to_str().unwrap()])
            .output()?;

        let _ = std::fs::remove_file(&script_path);

        Ok(gbk_to_utf8(&output.stdout))
    }

    /// 删除指定分区
    pub fn delete_partition(partition_letter: &str) -> Result<String> {
        let script_content = format!(
            "select volume {}\ndelete partition override",
            partition_letter.chars().next().unwrap_or('Y')
        );

        let temp_dir = std::env::temp_dir();
        let script_path = temp_dir.join("dp_delete.txt");
        std::fs::write(&script_path, &script_content)?;

        let output = create_command("diskpart")
            .args(["/s", script_path.to_str().unwrap()])
            .output()?;

        let _ = std::fs::remove_file(&script_path);

        Ok(gbk_to_utf8(&output.stdout))
    }

    /// 检查指定分区是否包含有效的 Windows 系统
    pub fn has_valid_windows(partition: &str) -> bool {
        let paths_to_check = [
            format!("{}\\Windows\\System32\\config\\SYSTEM", partition),
            format!("{}\\Windows\\System32\\config\\SOFTWARE", partition),
            format!("{}\\Windows\\explorer.exe", partition),
        ];

        paths_to_check.iter().all(|p| Path::new(p).exists())
    }

    /// 获取 Windows 版本信息
    pub fn get_windows_version(partition: &str) -> Option<String> {
        let ntoskrnl = format!("{}\\Windows\\System32\\ntoskrnl.exe", partition);
        if !Path::new(&ntoskrnl).exists() {
            return None;
        }

        // 尝试使用 wmic 获取版本信息
        let output = create_command("wmic")
            .args(["datafile", "where", &format!("name='{}'", ntoskrnl.replace("\\", "\\\\")), "get", "Version"])
            .output()
            .ok()?;

        let stdout = gbk_to_utf8(&output.stdout);
        stdout.lines()
            .skip(1)
            .next()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    }

    pub fn is_pe_environment() -> bool {
        crate::core::system_info::SystemInfo::check_pe_environment()
    }
}
