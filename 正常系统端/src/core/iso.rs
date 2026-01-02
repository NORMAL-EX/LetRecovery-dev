use anyhow::Result;
use std::path::Path;
use crate::utils::cmd::create_command;

#[cfg(windows)]
use windows::{
    core::PCWSTR,
    Win32::Foundation::{CloseHandle, HANDLE, WIN32_ERROR},
    Win32::Storage::Vhd::{
        AttachVirtualDisk, GetVirtualDiskPhysicalPath, OpenVirtualDisk,
        ATTACH_VIRTUAL_DISK_FLAG_PERMANENT_LIFETIME, ATTACH_VIRTUAL_DISK_FLAG_READ_ONLY,
        OPEN_VIRTUAL_DISK_FLAG_NONE, OPEN_VIRTUAL_DISK_PARAMETERS, OPEN_VIRTUAL_DISK_VERSION_1,
        VIRTUAL_DISK_ACCESS_READ, VIRTUAL_STORAGE_TYPE, VIRTUAL_STORAGE_TYPE_DEVICE_ISO,
    },
};

use crate::utils::encoding::gbk_to_utf8;

/// Microsoft Virtual Storage Type Vendor GUID
#[cfg(windows)]
const VIRTUAL_STORAGE_TYPE_VENDOR_MICROSOFT: windows::core::GUID = windows::core::GUID::from_u128(
    0xEC984AEC_A0F9_47e9_901F_71415A66345B,
);

pub struct IsoMounter {
    #[cfg(windows)]
    handle: Option<HANDLE>,
}

impl IsoMounter {
    pub fn new() -> Self {
        Self {
            #[cfg(windows)]
            handle: None,
        }
    }

    /// 检查是否在 PE 环境
    fn is_pe_environment() -> bool {
        crate::core::system_info::SystemInfo::check_pe_environment()
    }

    /// 使用 Windows API 挂载 ISO (Windows 8+)
    #[cfg(windows)]
    pub fn mount_iso_winapi(iso_path: &str) -> Result<()> {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;
        use windows::Win32::Storage::Vhd::{
            ATTACH_VIRTUAL_DISK_PARAMETERS, ATTACH_VIRTUAL_DISK_VERSION_1,
        };

        println!("[ISO] 使用 Windows API 挂载 ISO: {}", iso_path);

        // 转换路径为宽字符
        let wide_path: Vec<u16> = OsStr::new(iso_path)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        unsafe {
            // 设置存储类型为 ISO
            let storage_type = VIRTUAL_STORAGE_TYPE {
                DeviceId: VIRTUAL_STORAGE_TYPE_DEVICE_ISO,
                VendorId: VIRTUAL_STORAGE_TYPE_VENDOR_MICROSOFT,
            };

            // 设置打开参数 (ISO 必须使用 V1)
            let mut open_params: OPEN_VIRTUAL_DISK_PARAMETERS = std::mem::zeroed();
            open_params.Version = OPEN_VIRTUAL_DISK_VERSION_1;

            // 打开虚拟磁盘
            let mut handle: HANDLE = HANDLE::default();
            let result = OpenVirtualDisk(
                &storage_type,
                PCWSTR::from_raw(wide_path.as_ptr()),
                VIRTUAL_DISK_ACCESS_READ,
                OPEN_VIRTUAL_DISK_FLAG_NONE,
                Some(&open_params),
                &mut handle,
            );

            if result != WIN32_ERROR(0) {
                println!("[ISO] OpenVirtualDisk 失败: {:?}", result);
                anyhow::bail!("OpenVirtualDisk 失败: {:?}", result);
            }

            println!("[ISO] OpenVirtualDisk 成功, handle: {:?}", handle);

            // 设置挂载参数
            let mut attach_params: ATTACH_VIRTUAL_DISK_PARAMETERS = std::mem::zeroed();
            attach_params.Version = ATTACH_VIRTUAL_DISK_VERSION_1;

            // 挂载虚拟磁盘 (只读, 自动分配盘符, 永久生命周期)
            use windows::Win32::Storage::Vhd::ATTACH_VIRTUAL_DISK_FLAG;
            let attach_flags = ATTACH_VIRTUAL_DISK_FLAG(
                ATTACH_VIRTUAL_DISK_FLAG_READ_ONLY.0 | ATTACH_VIRTUAL_DISK_FLAG_PERMANENT_LIFETIME.0
            );

            let result = AttachVirtualDisk(
                handle,
                None, // 使用默认安全描述符
                attach_flags,
                0,    // 无特定提供程序标志
                Some(&attach_params),
                None, // 同步操作
            );

            if result != WIN32_ERROR(0) {
                println!("[ISO] AttachVirtualDisk 失败: {:?}", result);
                let _ = CloseHandle(handle);
                anyhow::bail!("AttachVirtualDisk 失败: {:?}", result);
            }

            println!("[ISO] AttachVirtualDisk 成功");

            // 获取挂载的物理路径 (可选，用于调试)
            let mut path_buffer = [0u16; 260];
            let mut path_size = (path_buffer.len() * 2) as u32;
            let result = GetVirtualDiskPhysicalPath(
                handle, 
                &mut path_size, 
                windows::core::PWSTR::from_raw(path_buffer.as_mut_ptr())
            );

            if result == WIN32_ERROR(0) {
                let path = String::from_utf16_lossy(&path_buffer[..path_size as usize / 2]);
                println!("[ISO] 物理路径: {}", path.trim_end_matches('\0'));
            }

            // 等待系统分配盘符
            std::thread::sleep(std::time::Duration::from_millis(1500));

            // 关闭句柄 (因为使用了 PERMANENT_LIFETIME，ISO 会保持挂载)
            let _ = CloseHandle(handle);

            Ok(())
        }
    }

    /// 使用 Windows API 卸载所有 ISO
    #[cfg(windows)]
    pub fn unmount_iso_winapi() -> Result<()> {
        println!("[ISO] 使用 PowerShell 卸载所有 ISO");

        // 使用 PowerShell 卸载所有已挂载的 ISO
        let ps_script = r#"
            Get-DiskImage | Where-Object { $_.ImagePath -like '*.iso' -and $_.Attached -eq $true } | ForEach-Object {
                Write-Host "卸载: $($_.ImagePath)"
                Dismount-DiskImage -ImagePath $_.ImagePath -ErrorAction SilentlyContinue
            }
        "#;

        let output = create_command("powershell.exe")
            .args(["-NoProfile", "-Command", ps_script])
            .output()?;

        let stdout = gbk_to_utf8(&output.stdout);
        println!("[ISO] PowerShell 输出: {}", stdout);

        Ok(())
    }

    /// 挂载 ISO (自动选择最佳方法)
    pub fn mount_iso(iso_path: &str) -> Result<()> {
        println!("[ISO] ========== 挂载 ISO ==========");
        println!("[ISO] 路径: {}", iso_path);

        // 先尝试卸载已存在的挂载
        let _ = Self::unmount();
        std::thread::sleep(std::time::Duration::from_millis(300));

        let is_pe = Self::is_pe_environment();
        println!("[ISO] PE 环境: {}", is_pe);

        // 方法1: 使用 Windows Virtual Disk API（PE和非PE都可用）
        #[cfg(windows)]
        {
            println!("[ISO] 尝试方法1: Windows Virtual Disk API");
            match Self::mount_iso_winapi(iso_path) {
                Ok(_) => {
                    // 查找挂载的盘符
                    if let Some(drive) = Self::find_iso_drive() {
                        println!("[ISO] Windows API 挂载成功，ISO 已挂载到: {}", drive);
                        return Ok(());
                    } else {
                        println!("[ISO] Windows API 挂载成功但未找到盘符，继续尝试其他方法");
                    }
                }
                Err(e) => {
                    println!("[ISO] Windows API 挂载失败: {}", e);
                }
            }
        }

        // 方法2: 使用 PowerShell Mount-DiskImage（PE和非PE都可用）
        {
            println!("[ISO] 尝试方法2: PowerShell Mount-DiskImage");

            let ps_script = format!(
                r#"
                $ErrorActionPreference = 'Stop'
                try {{
                    $result = Mount-DiskImage -ImagePath '{}' -PassThru
                    $volume = $result | Get-Volume
                    Write-Host "挂载成功: $($volume.DriveLetter):"
                }} catch {{
                    Write-Host "挂载失败: $_"
                    exit 1
                }}
                "#,
                iso_path.replace("'", "''")
            );

            let output = create_command("powershell.exe")
                .args(["-NoProfile", "-Command", &ps_script])
                .output()?;

            println!("[ISO] PowerShell stdout: {}", gbk_to_utf8(&output.stdout));
            println!("[ISO] PowerShell stderr: {}", gbk_to_utf8(&output.stderr));

            if output.status.success() {
                std::thread::sleep(std::time::Duration::from_millis(500));
                if let Some(drive) = Self::find_iso_drive() {
                    println!("[ISO] PowerShell 挂载成功到: {}", drive);
                    return Ok(());
                }
            }
        }

        anyhow::bail!(
            "ISO 挂载失败。请确保:\n\
             1. ISO 文件路径正确且文件完整\n\
             2. 系统支持虚拟磁盘 API (Windows 8+)\n\
             3. 或手动挂载 ISO 后重试"
        )
    }

    /// 卸载 ISO
    pub fn unmount() -> Result<()> {
        println!("[ISO] ========== 卸载 ISO ==========");

        // 使用 PowerShell 卸载所有已挂载的 ISO
        #[cfg(windows)]
        {
            let _ = Self::unmount_iso_winapi();
        }

        Ok(())
    }

    /// 查找已挂载的 ISO 驱动器盘符
    pub fn find_iso_drive() -> Option<String> {
        // 检查常用盘符
        for letter in ['Z', 'Y', 'X', 'W', 'V', 'U', 'D', 'E', 'F', 'G', 'H', 'I'] {
            let drive = format!("{}:", letter);
            let sources_path = format!("{}\\sources", drive);
            
            // 检查是否是 Windows 安装介质
            if Path::new(&sources_path).exists() {
                let install_wim = format!("{}\\install.wim", sources_path);
                let install_esd = format!("{}\\install.esd", sources_path);
                
                if Path::new(&install_wim).exists() || Path::new(&install_esd).exists() {
                    return Some(drive);
                }
            }
        }
        None
    }

    /// 在挂载的 ISO 中查找系统镜像文件
    pub fn find_install_image() -> Option<String> {
        // 先查找动态挂载的盘符
        if let Some(drive) = Self::find_iso_drive() {
            let paths = [
                format!("{}\\sources\\install.wim", drive),
                format!("{}\\sources\\install.esd", drive),
                format!("{}\\sources\\install.swm", drive),
            ];

            for path in &paths {
                if Path::new(path).exists() {
                    println!("[ISO] 找到安装镜像: {}", path);
                    return Some(path.clone());
                }
            }
        }

        // 后备: 检查固定的 Z: 盘
        let paths = [
            "Z:\\sources\\install.wim",
            "Z:\\sources\\install.esd",
            "Z:\\sources\\install.swm",
        ];

        for path in &paths {
            if Path::new(path).exists() {
                println!("[ISO] 找到安装镜像: {}", path);
                return Some(path.to_string());
            }
        }

        println!("[ISO] 未找到安装镜像");
        None
    }

    /// 检查 ISO 是否已挂载
    pub fn is_mounted() -> bool {
        Self::find_iso_drive().is_some() || Path::new("Z:\\").exists()
    }

    /// 获取挂载的 ISO 的卷标
    pub fn get_volume_label() -> Option<String> {
        let drive = Self::find_iso_drive().unwrap_or_else(|| "Z:".to_string());

        let output = create_command("vol").args([&drive]).output().ok()?;

        let stdout = gbk_to_utf8(&output.stdout);
        for line in stdout.lines() {
            if line.contains("卷是") || line.contains("Volume in drive") {
                if let Some(label) = line.split("是").last().or_else(|| line.split("is").last()) {
                    return Some(label.trim().to_string());
                }
            }
        }
        None
    }
}

impl Default for IsoMounter {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for IsoMounter {
    fn drop(&mut self) {
        #[cfg(windows)]
        if let Some(handle) = self.handle.take() {
            unsafe {
                let _ = CloseHandle(handle);
            }
        }
    }
}
