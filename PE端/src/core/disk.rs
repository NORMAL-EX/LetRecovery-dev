use anyhow::Result;
use std::path::Path;
use std::process::Command;
use windows::core::PCWSTR;
use windows::Win32::Storage::FileSystem::{GetDiskFreeSpaceExW, GetDriveTypeW, GetVolumeInformationW};

use crate::utils::encoding::gbk_to_utf8;
use crate::utils::path::get_bin_dir;

const DRIVE_FIXED: u32 = 3;

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

        for letter in b'A'..=b'Z' {
            let drive = format!("{}:", letter as char);
            if let Ok(info) = Self::get_partition_info(&drive) {
                partitions.push(info);
            }
        }

        Ok(partitions)
    }

    fn get_partition_info(drive: &str) -> Result<Partition> {
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

        // PE环境下排除 X: 盘
        let system_drive = std::env::var("SystemDrive").unwrap_or_else(|_| "X:".to_string());
        let is_current_system = drive.eq_ignore_ascii_case(&system_drive);

        // 检查是否包含 Windows 系统
        let windows_path = format!("{}\\Windows\\System32", drive);
        let has_windows = Path::new(&windows_path).exists();

        // PE环境下，is_system_partition 表示是否包含 Windows（排除PE自己的X盘）
        let is_system_partition = has_windows && !is_current_system;

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

        let output = Command::new("powershell")
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

        let output = match Command::new("diskpart")
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

        let mut disk_num: Option<u32> = None;
        let mut part_num: Option<u32> = None;

        for line in stdout.lines() {
            let line_upper = line.to_uppercase();
            if (line_upper.contains("磁盘") || line_upper.contains("DISK"))
                && !line_upper.contains("磁盘 ID")
                && !line_upper.contains("DISK ID")
            {
                if let Some(num) = line
                    .split_whitespace()
                    .find(|s| s.parse::<u32>().is_ok())
                {
                    disk_num = num.parse().ok();
                }
            }
            if line_upper.contains("分区") || line_upper.contains("PARTITION") {
                if let Some(num) = line
                    .split_whitespace()
                    .find(|s| s.parse::<u32>().is_ok())
                {
                    part_num = num.parse().ok();
                }
            }
        }

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

        let output = match Command::new("diskpart")
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
        log::info!("格式化分区: {}", partition);

        let bin_dir = get_bin_dir();
        let format_exe = bin_dir.join("format.com").to_string_lossy().to_string();

        // 优先使用 bin 目录下的 format.com，如果不存在则使用系统的
        let format_cmd = if Path::new(&format_exe).exists() {
            format_exe
        } else {
            "format.com".to_string()
        };

        let output = Command::new(&format_cmd)
            .args([partition, "/FS:NTFS", "/q", "/y"])
            .output()?;

        let result = gbk_to_utf8(&output.stdout);
        log::info!("格式化结果: {}", result);

        if !output.status.success() {
            let stderr = gbk_to_utf8(&output.stderr);
            anyhow::bail!("格式化失败: {}", stderr);
        }

        Ok(result)
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

    /// 检测是否为UEFI模式
    pub fn detect_uefi_mode() -> bool {
        // 检查EFI系统分区
        for letter in ['S', 'T', 'U', 'V', 'W', 'Y', 'Z'] {
            let efi_path = format!("{}:\\EFI\\Microsoft\\Boot", letter);
            if Path::new(&efi_path).exists() {
                return true;
            }
        }

        // 检查固件类型
        let output = Command::new("cmd")
            .args(["/c", "bcdedit /enum firmware"])
            .output();

        if let Ok(output) = output {
            let stdout = gbk_to_utf8(&output.stdout);
            if stdout.contains("firmware") || stdout.contains("UEFI") {
                return true;
            }
        }

        // 检查 SecureBoot 变量
        let output = Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "Confirm-SecureBootUEFI -ErrorAction SilentlyContinue",
            ])
            .output();

        if let Ok(output) = output {
            let stdout = gbk_to_utf8(&output.stdout).trim().to_lowercase();
            if stdout == "true" {
                return true;
            }
        }

        false
    }
}
