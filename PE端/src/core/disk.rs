use anyhow::Result;
use std::path::Path;
use windows::core::PCWSTR;
use windows::Win32::Storage::FileSystem::{GetDiskFreeSpaceExW, GetDriveTypeW, GetVolumeInformationW};

use crate::utils::command::new_command;
use crate::utils::encoding::gbk_to_utf8;
use crate::utils::path::get_bin_dir;

const DRIVE_FIXED: u32 = 3;

/// 自动创建分区的标志文件名
pub const AUTO_CREATED_PARTITION_MARKER: &str = "LetRecovery_AutoCreated.marker";

/// 获取 diskpart 可执行文件路径
/// 优先使用内置的 diskpart，如果不存在则使用系统的
fn get_diskpart_path() -> String {
    let builtin_diskpart = get_bin_dir().join("diskpart").join("diskpart.exe");
    if builtin_diskpart.exists() {
        log::info!("使用内置 diskpart: {}", builtin_diskpart.display());
        builtin_diskpart.to_string_lossy().to_string()
    } else {
        log::info!("使用系统 diskpart");
        "diskpart.exe".to_string()
    }
}

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
        // PE环境下直接使用 diskpart
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

        let output = match new_command(&get_diskpart_path())
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

        let output = match new_command(&get_diskpart_path())
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

        let output = new_command(&format_cmd)
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
        let output = new_command("cmd")
            .args(["/c", "bcdedit /enum firmware"])
            .output();

        if let Ok(output) = output {
            let stdout = gbk_to_utf8(&output.stdout);
            if stdout.contains("firmware") || stdout.contains("UEFI") {
                return true;
            }
        }

        false
    }

    /// 查找自动创建的分区（通过标志文件）
    /// 返回 (盘符, 磁盘号Option) 如果找到的话
    pub fn find_auto_created_partition() -> Option<(char, Option<u32>)> {
        for letter in b'A'..=b'Z' {
            let c = letter as char;
            // 跳过 X 盘（PE系统盘）
            if c == 'X' {
                continue;
            }
            
            let marker_path = format!("{}:\\{}", c, AUTO_CREATED_PARTITION_MARKER);
            if Path::new(&marker_path).exists() {
                log::info!("找到自动创建的分区: {}:", c);
                
                // 获取该分区所在的磁盘号
                let detail = Self::get_partition_style(&format!("{}:", c));
                return Some((c, detail.disk_number));
            }
        }
        None
    }

    /// 检查指定分区是否是自动创建的
    pub fn is_auto_created_partition(letter: char) -> bool {
        let marker_path = format!("{}:\\{}", letter, AUTO_CREATED_PARTITION_MARKER);
        Path::new(&marker_path).exists()
    }

    /// 删除自动创建的分区并扩展目标分区
    /// 
    /// # Arguments
    /// * `target_partition` - 目标安装分区（如 "D:"），删除数据分区后要扩展的分区
    /// 
    /// 流程：
    /// 1. 找到自动创建的分区
    /// 2. 确认该分区和目标分区在同一个磁盘上
    /// 3. 删除该分区
    /// 4. 扩展目标分区以使用释放的空间
    pub fn cleanup_auto_created_partition_and_extend(target_partition: &str) -> Result<()> {
        let target_letter = target_partition.chars().next().unwrap_or('C').to_ascii_uppercase();
        
        log::info!("[CLEANUP] 检查是否有自动创建的分区需要清理...");
        log::info!("[CLEANUP] 目标安装分区: {}:", target_letter);

        // 查找自动创建的分区
        let (auto_letter, auto_disk_num_opt) = match Self::find_auto_created_partition() {
            Some(info) => info,
            None => {
                log::info!("[CLEANUP] 未找到自动创建的分区，无需清理");
                return Ok(());
            }
        };

        // 如果无法获取自动创建分区的磁盘号，只删除不扩展
        let auto_disk_num = match auto_disk_num_opt {
            Some(num) => num,
            None => {
                log::warn!("[CLEANUP] 无法获取自动创建分区 {} 的磁盘号，只删除不扩展", auto_letter);
                return Self::delete_partition_by_letter(auto_letter);
            }
        };

        log::info!(
            "[CLEANUP] 找到自动创建的分区 {}: 在磁盘 {}",
            auto_letter, auto_disk_num
        );

        // 获取目标分区所在的磁盘号
        let target_detail = Self::get_partition_style(&format!("{}:", target_letter));
        let target_disk_num = match target_detail.disk_number {
            Some(num) => num,
            None => {
                log::warn!("[CLEANUP] 无法获取目标分区 {} 的磁盘号，只删除分区不扩展", target_letter);
                // 无法判断是否同一磁盘，安全起见只删除不扩展
                return Self::delete_partition_by_letter(auto_letter);
            }
        };

        log::info!("[CLEANUP] 目标分区 {} 在磁盘 {}", target_letter, target_disk_num);

        // 检查是否在同一磁盘
        if auto_disk_num != target_disk_num {
            log::warn!(
                "[CLEANUP] 自动创建的分区 ({}) 和目标分区 ({}) 不在同一磁盘，只删除分区不扩展",
                auto_letter, target_letter
            );
            // 只删除分区，不扩展
            return Self::delete_partition_by_letter(auto_letter);
        }

        // 删除自动创建分区并扩展目标分区
        log::info!("[CLEANUP] 开始删除分区 {} 并扩展目标分区 {}...", auto_letter, target_letter);
        Self::delete_partition_and_extend(auto_letter, target_letter)
    }

    /// 删除指定盘符的分区
    fn delete_partition_by_letter(letter: char) -> Result<()> {
        log::info!("[CLEANUP] 删除分区 {}:", letter);

        let script_content = format!(
            "select volume {}\ndelete partition override",
            letter
        );

        let temp_dir = std::env::temp_dir();
        let script_path = temp_dir.join("lr_delete_part.txt");
        std::fs::write(&script_path, &script_content)?;

        let output = new_command(&get_diskpart_path())
            .args(["/s", script_path.to_str().unwrap()])
            .output()?;

        let _ = std::fs::remove_file(&script_path);

        let output_text = gbk_to_utf8(&output.stdout);
        log::info!("[CLEANUP] Diskpart 删除输出: {}", output_text);

        // 检查是否有错误
        let output_lower = output_text.to_lowercase();
        if output_lower.contains("error") || output_lower.contains("错误")
            || output_lower.contains("失败") || output_lower.contains("failed") {
            anyhow::bail!("删除分区失败: {}", output_text);
        }

        log::info!("[CLEANUP] 分区 {} 删除成功", letter);
        Ok(())
    }

    /// 删除分区并扩展目标分区
    fn delete_partition_and_extend(auto_letter: char, target_letter: char) -> Result<()> {
        // Step 1: 删除分区
        log::info!("[CLEANUP] Step 1: 删除分区 {}:", auto_letter);
        
        let delete_script = format!(
            "select volume {}\ndelete partition override",
            auto_letter
        );

        let temp_dir = std::env::temp_dir();
        let script_path = temp_dir.join("lr_delete_part.txt");
        std::fs::write(&script_path, &delete_script)?;

        let output = new_command(&get_diskpart_path())
            .args(["/s", script_path.to_str().unwrap()])
            .output()?;

        let _ = std::fs::remove_file(&script_path);

        let output_text = gbk_to_utf8(&output.stdout);
        log::info!("[CLEANUP] 删除分区输出: {}", output_text);

        // 检查删除是否成功
        let output_lower = output_text.to_lowercase();
        if output_lower.contains("error") || output_lower.contains("错误")
            || output_lower.contains("失败") || output_lower.contains("failed") {
            // 删除失败，直接返回错误
            anyhow::bail!("删除分区失败: {}", output_text);
        }

        log::info!("[CLEANUP] 分区 {} 删除成功", auto_letter);

        // Step 2: 等待系统识别未分配空间，然后扩展目标分区（带重试）
        log::info!("[CLEANUP] Step 2: 扩展目标分区 {}（带重试）", target_letter);
        
        const MAX_RETRIES: u32 = 5;
        const RETRY_DELAY_SECS: u64 = 2;
        
        for attempt in 1..=MAX_RETRIES {
            log::info!("[CLEANUP] 扩展分区 {} 尝试 {}/{}", target_letter, attempt, MAX_RETRIES);
            
            // 等待系统识别未分配空间
            std::thread::sleep(std::time::Duration::from_secs(RETRY_DELAY_SECS));
            
            // 尝试扩展
            match Self::try_extend_volume(target_letter) {
                Ok(_) => {
                    log::info!("[CLEANUP] 分区 {} 扩展成功！", target_letter);
                    return Ok(());
                }
                Err(e) => {
                    log::warn!("[CLEANUP] 扩展尝试 {} 失败: {}", attempt, e);
                    if attempt < MAX_RETRIES {
                        log::info!("[CLEANUP] 等待 {} 秒后重试...", RETRY_DELAY_SECS);
                    }
                }
            }
        }

        // 所有重试都失败了
        log::warn!("[CLEANUP] 分区 {} 扩展失败，但数据分区已删除。用户可在系统安装完成后手动扩展。", target_letter);
        Ok(())
    }

    /// 尝试扩展指定分区（使用 diskpart）
    fn try_extend_volume(letter: char) -> Result<()> {
        let extend_script = format!("select volume {}\nextend", letter);
        
        let temp_dir = std::env::temp_dir();
        let script_path = temp_dir.join("lr_extend.txt");
        std::fs::write(&script_path, &extend_script)?;

        let output = new_command(&get_diskpart_path())
            .args(["/s", script_path.to_str().unwrap()])
            .output()?;

        let _ = std::fs::remove_file(&script_path);

        let output_text = gbk_to_utf8(&output.stdout);
        let output_lower = output_text.to_lowercase();

        log::info!("[CLEANUP] diskpart extend 输出: {}", output_text);

        // 检查是否成功
        if output_lower.contains("成功") || output_lower.contains("successfully") {
            return Ok(());
        }

        // 检查是否有错误
        if output_lower.contains("error") || output_lower.contains("错误")
            || output_lower.contains("失败") || output_lower.contains("failed")
            || output_lower.contains("没有可用") || output_lower.contains("no usable") {
            anyhow::bail!("extend 失败: {}", output_text);
        }

        // 不确定状态，假设失败
        anyhow::bail!("extend 状态不确定: {}", output_text)
    }
}
