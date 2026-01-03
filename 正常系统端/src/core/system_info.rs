use anyhow::Result;

#[cfg(windows)]
use windows::{
    core::PCWSTR,
    Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_LOCAL_MACHINE, KEY_READ, REG_DWORD, REG_SZ,
    },
};

#[derive(Debug, Clone)]
pub struct SystemInfo {
    pub boot_mode: BootMode,
    pub tpm_enabled: bool,
    pub tpm_version: String,
    pub secure_boot: bool,
    pub is_pe_environment: bool,
    pub is_64bit: bool,
    pub is_online: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BootMode {
    UEFI,
    Legacy,
}

impl std::fmt::Display for BootMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BootMode::UEFI => write!(f, "UEFI"),
            BootMode::Legacy => write!(f, "Legacy"),
        }
    }
}

/// 直接调用 kernel32.dll 的 GetFirmwareEnvironmentVariableW
#[cfg(windows)]
mod kernel32 {
    #[link(name = "kernel32")]
    extern "system" {
        pub fn GetFirmwareEnvironmentVariableW(
            lpName: *const u16,
            lpGuid: *const u16,
            pBuffer: *mut u8,
            nSize: u32,
        ) -> u32;
    }
}

impl SystemInfo {
    pub fn collect() -> Result<Self> {
        let is_pe = Self::check_pe_environment();
        let boot_mode = Self::get_boot_mode()?;
        let (tpm_enabled, tpm_version) = Self::get_tpm_info();
        let secure_boot = Self::get_secure_boot().unwrap_or(false);
        let is_online = Self::check_network();

        Ok(Self {
            boot_mode,
            tpm_enabled,
            tpm_version,
            secure_boot,
            is_pe_environment: is_pe,
            is_64bit: cfg!(target_arch = "x86_64"),
            is_online,
        })
    }

    /// 使用 Windows API 检测启动模式
    #[cfg(windows)]
    fn get_boot_mode() -> Result<BootMode> {
        // 方法1: 检查 EFI 系统分区特征文件/目录
        if std::path::Path::new("\\EFI").exists()
            || std::path::Path::new("C:\\EFI").exists()
            || std::path::Path::new("X:\\EFI").exists()
        {
            return Ok(BootMode::UEFI);
        }

        // 方法2: 使用 GetFirmwareEnvironmentVariableW API
        // 这个 API 在 Legacy BIOS 下会返回 ERROR_INVALID_FUNCTION (1)
        unsafe {
            let name: Vec<u16> = "".encode_utf16().chain(std::iter::once(0)).collect();
            let guid: Vec<u16> = "{00000000-0000-0000-0000-000000000000}"
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            let mut buffer = [0u8; 1];

            let result = kernel32::GetFirmwareEnvironmentVariableW(
                name.as_ptr(),
                guid.as_ptr(),
                buffer.as_mut_ptr(),
                buffer.len() as u32,
            );

            // 如果返回 0，检查错误码
            if result == 0 {
                let error = std::io::Error::last_os_error();
                let raw_error = error.raw_os_error().unwrap_or(0) as u32;
                
                // ERROR_INVALID_FUNCTION (1) 表示是 Legacy BIOS
                if raw_error == 1 {
                    return Ok(BootMode::Legacy);
                }
                // 其他错误（如 ERROR_NOACCESS 998）表示是 UEFI，只是没有访问权限
                return Ok(BootMode::UEFI);
            }

            // 如果调用成功，说明是 UEFI
            Ok(BootMode::UEFI)
        }
    }

    #[cfg(not(windows))]
    fn get_boot_mode() -> Result<BootMode> {
        Ok(BootMode::Legacy)
    }

    /// 获取 TPM 信息（使用注册表）
    #[cfg(windows)]
    fn get_tpm_info() -> (bool, String) {
        // 方法1: 检查 TPM 服务注册表键
        let tpm_present = Self::check_tpm_registry();
        
        if tpm_present {
            // 尝试获取版本
            let version = Self::get_tpm_version_from_registry();
            (true, version)
        } else {
            (false, String::new())
        }
    }

    #[cfg(not(windows))]
    fn get_tpm_info() -> (bool, String) {
        (false, String::new())
    }

    /// 检查 TPM 是否存在（通过注册表）
    #[cfg(windows)]
    fn check_tpm_registry() -> bool {
        unsafe {
            // 检查 TPM 服务
            let subkey: Vec<u16> = "SYSTEM\\CurrentControlSet\\Services\\TPM"
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();

            let mut hkey = HKEY::default();
            let result = RegOpenKeyExW(
                HKEY_LOCAL_MACHINE,
                PCWSTR::from_raw(subkey.as_ptr()),
                0,
                KEY_READ,
                &mut hkey,
            );

            if result.is_ok() {
                let _ = RegCloseKey(hkey);
                return true;
            }

            // 备选：检查 TPM 设备注册表 (TPM 2.0)
            let subkey: Vec<u16> = "SYSTEM\\CurrentControlSet\\Enum\\ACPI\\MSFT0101"
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();

            let result = RegOpenKeyExW(
                HKEY_LOCAL_MACHINE,
                PCWSTR::from_raw(subkey.as_ptr()),
                0,
                KEY_READ,
                &mut hkey,
            );

            if result.is_ok() {
                let _ = RegCloseKey(hkey);
                return true;
            }

            false
        }
    }

    /// 从注册表获取 TPM 版本
    #[cfg(windows)]
    fn get_tpm_version_from_registry() -> String {
        unsafe {
            // 尝试从 TPM 注册表读取版本信息
            let subkey: Vec<u16> = "SOFTWARE\\Microsoft\\Tpm"
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();

            let mut hkey = HKEY::default();
            let result = RegOpenKeyExW(
                HKEY_LOCAL_MACHINE,
                PCWSTR::from_raw(subkey.as_ptr()),
                0,
                KEY_READ,
                &mut hkey,
            );

            if result.is_ok() {
                // 尝试读取 SpecVersion
                let value_name: Vec<u16> = "SpecVersion"
                    .encode_utf16()
                    .chain(std::iter::once(0))
                    .collect();
                
                let mut buffer = [0u8; 256];
                let mut buffer_size = buffer.len() as u32;
                let mut value_type = REG_SZ;

                let result = RegQueryValueExW(
                    hkey,
                    PCWSTR::from_raw(value_name.as_ptr()),
                    None,
                    Some(&mut value_type),
                    Some(buffer.as_mut_ptr()),
                    Some(&mut buffer_size),
                );

                let _ = RegCloseKey(hkey);

                if result.is_ok() && buffer_size > 0 {
                    let wide_str: &[u16] = std::slice::from_raw_parts(
                        buffer.as_ptr() as *const u16,
                        (buffer_size as usize / 2).saturating_sub(1),
                    );
                    let version = String::from_utf16_lossy(wide_str);
                    // 取逗号前的第一部分
                    return version.split(',').next().unwrap_or("").trim().to_string();
                }
            }

            // 通过设备枚举检测版本
            // TPM 2.0 设备路径
            let subkey_20: Vec<u16> = "SYSTEM\\CurrentControlSet\\Enum\\ACPI\\MSFT0101"
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();

            let result = RegOpenKeyExW(
                HKEY_LOCAL_MACHINE,
                PCWSTR::from_raw(subkey_20.as_ptr()),
                0,
                KEY_READ,
                &mut hkey,
            );

            if result.is_ok() {
                let _ = RegCloseKey(hkey);
                return "2.0".to_string();
            }

            // 默认返回 2.0（现代系统大多是 TPM 2.0）
            "2.0".to_string()
        }
    }

    /// 使用注册表 API 检测安全启动状态
    #[cfg(windows)]
    fn get_secure_boot() -> Result<bool> {
        unsafe {
            let subkey: Vec<u16> = "SYSTEM\\CurrentControlSet\\Control\\SecureBoot\\State"
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();

            let mut hkey = HKEY::default();
            let result = RegOpenKeyExW(
                HKEY_LOCAL_MACHINE,
                PCWSTR::from_raw(subkey.as_ptr()),
                0,
                KEY_READ,
                &mut hkey,
            );

            if result.is_err() {
                return Ok(false);
            }

            let value_name: Vec<u16> = "UEFISecureBootEnabled"
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();

            let mut data = [0u32; 1];
            let mut data_size = std::mem::size_of::<u32>() as u32;
            let mut data_type = REG_DWORD;

            let result = RegQueryValueExW(
                hkey,
                PCWSTR::from_raw(value_name.as_ptr()),
                None,
                Some(&mut data_type),
                Some(data.as_mut_ptr() as *mut u8),
                Some(&mut data_size),
            );

            let _ = RegCloseKey(hkey);

            if result.is_ok() {
                Ok(data[0] == 1)
            } else {
                Ok(false)
            }
        }
    }

    #[cfg(not(windows))]
    fn get_secure_boot() -> Result<bool> {
        Ok(false)
    }

    pub fn check_pe_environment() -> bool {
        // 特征1: fbwf.sys (File-Based Write Filter)
        if std::path::Path::new("X:\\Windows\\System32\\drivers\\fbwf.sys").exists() {
            return true;
        }

        // 特征2: winpeshl.ini
        if std::path::Path::new("X:\\Windows\\System32\\winpeshl.ini").exists() {
            return true;
        }

        // 特征3: 系统盘是 X:
        if let Ok(system_drive) = std::env::var("SystemDrive") {
            if system_drive.to_uppercase() == "X:" {
                return true;
            }
        }

        // 特征4: 检查 MININT 目录
        if std::path::Path::new("X:\\MININT").exists() {
            return true;
        }

        // 特征5: 检查 MiniNT 注册表键
        #[cfg(windows)]
        {
            if Self::check_minint_registry() {
                return true;
            }
        }

        // 特征6: 检查 SystemDrive 下的 PE 特征文件
        if let Ok(system_drive) = std::env::var("SystemDrive") {
            let fbwf_path = format!("{}\\Windows\\System32\\drivers\\fbwf.sys", system_drive);
            let winpeshl_path = format!("{}\\Windows\\System32\\winpeshl.ini", system_drive);
            if std::path::Path::new(&fbwf_path).exists()
                || std::path::Path::new(&winpeshl_path).exists()
            {
                return true;
            }
        }

        false
    }

    /// 检查 MiniNT 注册表键（PE 环境特征）
    #[cfg(windows)]
    fn check_minint_registry() -> bool {
        unsafe {
            let subkey: Vec<u16> = "SYSTEM\\CurrentControlSet\\Control\\MiniNT"
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();

            let mut hkey = HKEY::default();
            let result = RegOpenKeyExW(
                HKEY_LOCAL_MACHINE,
                PCWSTR::from_raw(subkey.as_ptr()),
                0,
                KEY_READ,
                &mut hkey,
            );

            if result.is_ok() {
                let _ = RegCloseKey(hkey);
                return true;
            }

            false
        }
    }

    fn check_network() -> bool {
        let addresses = [
            "223.5.5.5:53",
            "119.29.29.29:53",
            "8.8.8.8:53",
            "1.1.1.1:53",
        ];

        for addr in &addresses {
            if let Ok(addr) = addr.parse() {
                if std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_secs(2))
                    .is_ok()
                {
                    return true;
                }
            }
        }

        false
    }
}
