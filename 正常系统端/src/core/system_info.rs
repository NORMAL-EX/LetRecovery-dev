use anyhow::Result;
use crate::utils::cmd::create_command;

use crate::utils::encoding::gbk_to_utf8;

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

impl SystemInfo {
    pub fn collect() -> Result<Self> {
        let is_pe = Self::check_pe_environment();
        let boot_mode = Self::get_boot_mode(is_pe)?;
        
        // 在PE环境下使用不同的检测方法
        let (tpm_enabled, tpm_version) = if is_pe {
            Self::get_tpm_info_pe()
        } else {
            (
                Self::get_tpm_enabled().unwrap_or(false),
                Self::get_tpm_version().unwrap_or_default(),
            )
        };
        
        let secure_boot = Self::get_secure_boot(is_pe).unwrap_or(false);
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

    fn get_boot_mode(is_pe: bool) -> Result<BootMode> {
        // 方法1: 检查 EFI 系统分区特征文件/目录
        if std::path::Path::new("\\EFI").exists() 
            || std::path::Path::new("C:\\EFI").exists()
            || std::path::Path::new("X:\\EFI").exists() 
        {
            return Ok(BootMode::UEFI);
        }

        // 方法2: 使用 bcdedit 检查引导类型
        let output = create_command("bcdedit")
            .args(["/enum", "{current}"])
            .output();
        
        if let Ok(output) = output {
            let stdout = gbk_to_utf8(&output.stdout);
            if stdout.to_lowercase().contains("winload.efi") {
                return Ok(BootMode::UEFI);
            }
            if stdout.to_lowercase().contains("winload.exe") {
                return Ok(BootMode::Legacy);
            }
        }

        // 方法3: 非PE环境下尝试 PowerShell
        if !is_pe {
            let output = create_command("powershell")
                .args(["-Command", "$env:firmware_type"])
                .output();
            
            if let Ok(output) = output {
                let result = gbk_to_utf8(&output.stdout).trim().to_uppercase();
                if result == "UEFI" {
                    return Ok(BootMode::UEFI);
                } else if result == "BIOS" || result == "LEGACY" {
                    return Ok(BootMode::Legacy);
                }
            }
        }

        // 方法4: 检查 Windows Boot Manager 的 path
        let output = create_command("bcdedit")
            .args(["/enum", "{bootmgr}"])
            .output();
        
        if let Ok(output) = output {
            let stdout = gbk_to_utf8(&output.stdout);
            if stdout.to_lowercase().contains("\\efi\\") {
                return Ok(BootMode::UEFI);
            }
        }

        Ok(BootMode::Legacy)
    }

    /// PE环境下的TPM检测 - 使用wmic替代方案
    fn get_tpm_info_pe() -> (bool, String) {
        // 方法1: 尝试使用 wmic
        let output = create_command("wmic")
            .args(["/namespace:\\\\root\\cimv2\\security\\microsofttpm", "path", "Win32_Tpm", "get", "IsEnabled_InitialValue,SpecVersion"])
            .output();
        
        if let Ok(output) = output {
            let stdout = gbk_to_utf8(&output.stdout);
            if stdout.to_uppercase().contains("TRUE") {
                let version = stdout
                    .lines()
                    .skip(1)
                    .next()
                    .and_then(|line| {
                        let parts: Vec<&str> = line.split_whitespace().collect();
                        parts.get(1).map(|v| {
                            v.split(',').next().unwrap_or("").trim().to_string()
                        })
                    })
                    .unwrap_or_default();
                return (true, version);
            }
        }

        // 方法2: 检查 TPM 设备是否存在
        let output = create_command("wmic")
            .args(["path", "Win32_PnPEntity", "where", "Caption like '%TPM%'", "get", "Caption"])
            .output();
        
        if let Ok(output) = output {
            let stdout = gbk_to_utf8(&output.stdout);
            if stdout.to_lowercase().contains("tpm") {
                if stdout.contains("2.0") {
                    return (true, "2.0".to_string());
                } else if stdout.contains("1.2") {
                    return (true, "1.2".to_string());
                }
                return (true, String::new());
            }
        }

        (false, String::new())
    }

    fn get_tpm_enabled() -> Result<bool> {
        let output = create_command("powershell")
            .args(["-Command", "(Get-Tpm).TpmPresent"])
            .output()?;

        let result = gbk_to_utf8(&output.stdout).trim().to_lowercase();
        if result == "true" {
            return Ok(true);
        }
        if result == "false" {
            return Ok(false);
        }

        // 备用方法: wmic
        let output = create_command("wmic")
            .args(["/namespace:\\\\root\\cimv2\\security\\microsofttpm", "path", "Win32_Tpm", "get", "IsEnabled_InitialValue"])
            .output()?;
        
        Ok(gbk_to_utf8(&output.stdout).to_uppercase().contains("TRUE"))
    }

    fn get_tpm_version() -> Result<String> {
        let output = create_command("powershell")
            .args([
                "-Command",
                "((Get-WmiObject -Namespace root\\cimv2\\security\\microsofttpm -Class Win32_Tpm).SpecVersion -split ',')[0].Trim()",
            ])
            .output()?;

        let version = gbk_to_utf8(&output.stdout).trim().to_string();
        if !version.is_empty() && !version.contains("错误") && !version.contains("error") {
            return Ok(version);
        }

        // 备用方法: wmic
        let output = create_command("wmic")
            .args(["/namespace:\\\\root\\cimv2\\security\\microsofttpm", "path", "Win32_Tpm", "get", "SpecVersion"])
            .output()?;
        
        let stdout = gbk_to_utf8(&output.stdout);
        let version = stdout
            .lines()
            .skip(1)
            .next()
            .and_then(|line| line.split(',').next())
            .map(|v| v.trim().to_string())
            .unwrap_or_default();
        
        Ok(version)
    }

    fn get_secure_boot(is_pe: bool) -> Result<bool> {
        // 方法1: 检查注册表 (PE和正常系统都可用)
        let output = create_command("reg")
            .args(["query", "HKLM\\SYSTEM\\CurrentControlSet\\Control\\SecureBoot\\State", "/v", "UEFISecureBootEnabled"])
            .output();
        
        if let Ok(output) = output {
            let stdout = gbk_to_utf8(&output.stdout);
            if stdout.contains("0x1") {
                return Ok(true);
            }
            if stdout.contains("0x0") {
                return Ok(false);
            }
        }

        // 方法2: 非PE环境下尝试 PowerShell
        if !is_pe {
            let output = create_command("powershell")
                .args(["-Command", "Confirm-SecureBootUEFI"])
                .output()?;

            let result = gbk_to_utf8(&output.stdout).trim().to_lowercase();
            return Ok(result == "true");
        }

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
        
        // 特征5: 检查启动配置中的 winpe 标志
        if let Ok(output) = create_command("bcdedit").args(["/enum", "{current}"]).output() {
            let stdout = gbk_to_utf8(&output.stdout).to_lowercase();
            if stdout.contains("winpe") && stdout.contains("yes") {
                return true;
            }
        }
        
        // 特征6: 检查 SystemDrive 下的 PE 特征文件
        if let Ok(system_drive) = std::env::var("SystemDrive") {
            let fbwf_path = format!("{}\\Windows\\System32\\drivers\\fbwf.sys", system_drive);
            let winpeshl_path = format!("{}\\Windows\\System32\\winpeshl.ini", system_drive);
            if std::path::Path::new(&fbwf_path).exists() 
                || std::path::Path::new(&winpeshl_path).exists() {
                return true;
            }
        }

        false
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
                if std::net::TcpStream::connect_timeout(
                    &addr,
                    std::time::Duration::from_secs(2),
                ).is_ok() {
                    return true;
                }
            }
        }
        
        // 备用方法: ping
        let output = create_command("ping")
            .args(["-n", "1", "-w", "1000", "223.5.5.5"])
            .output();
        
        if let Ok(output) = output {
            return output.status.success();
        }

        false
    }
}
