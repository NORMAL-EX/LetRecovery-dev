//! 硬件信息模块
//! 使用纯 WinAPI 获取硬件信息，无需依赖外部工具或命令行
//! 
//! 主要使用以下 API:
//! - GetSystemInfo / GetNativeSystemInfo - CPU 基本信息
//! - GlobalMemoryStatusEx - 内存信息
//! - Registry API - CPU 详细信息、BIOS/主板信息
//! - DeviceIoControl - 硬盘信息
//! - EnumDisplayDevices - 显示适配器信息

use std::ffi::OsString;
use std::mem::{size_of, zeroed};
use std::os::windows::ffi::OsStringExt;

use windows::core::PCWSTR;
use windows::Win32::Foundation::{BOOL, CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayDevicesW, EnumDisplaySettingsW, DEVMODEW, DISPLAY_DEVICEW, 
    ENUM_CURRENT_SETTINGS,
};
use windows::Win32::System::Registry::{
    RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_LOCAL_MACHINE, 
    KEY_READ, REG_VALUE_TYPE,
};
use windows::Win32::System::SystemInformation::{
    GetNativeSystemInfo, GlobalMemoryStatusEx, MEMORYSTATUSEX, SYSTEM_INFO,
};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    FILE_FLAGS_AND_ATTRIBUTES,
};
use windows::Win32::System::IO::DeviceIoControl;
use windows::Win32::System::Ioctl::{
    IOCTL_STORAGE_QUERY_PROPERTY, IOCTL_DISK_GET_LENGTH_INFO,
    STORAGE_PROPERTY_QUERY, StorageDeviceProperty, PropertyStandardQuery,
};

/// CPU 信息
#[derive(Debug, Clone, Default)]
pub struct CpuInfo {
    pub name: String,
    pub manufacturer: String,
    pub cores: u32,
    pub logical_processors: u32,
    pub max_clock_speed: u32, // MHz
    pub current_clock_speed: u32, // MHz
    pub l2_cache_size: u32, // KB
    pub l3_cache_size: u32, // KB
    pub architecture: String,
}

/// 内存条信息
#[derive(Debug, Clone, Default)]
pub struct MemoryStickInfo {
    pub capacity: u64, // Bytes
    pub speed: u32, // MHz
    pub manufacturer: String,
    pub part_number: String,
    pub bank_label: String,
    pub device_locator: String,
    pub memory_type: String,
}

/// 内存信息
#[derive(Debug, Clone, Default)]
pub struct MemoryInfo {
    pub total_physical: u64, // Bytes
    pub available_physical: u64, // Bytes
    pub total_virtual: u64, // Bytes
    pub available_virtual: u64, // Bytes
    pub memory_load: u32, // 使用百分比
    pub sticks: Vec<MemoryStickInfo>,
}

/// 主板信息
#[derive(Debug, Clone, Default)]
pub struct MotherboardInfo {
    pub manufacturer: String,
    pub product: String,
    pub version: String,
    pub serial_number: String,
}

/// BIOS 信息
#[derive(Debug, Clone, Default)]
pub struct BiosInfo {
    pub manufacturer: String,
    pub version: String,
    pub release_date: String,
    pub smbios_version: String,
}

/// 硬盘信息
#[derive(Debug, Clone, Default)]
pub struct DiskInfo {
    pub model: String,
    pub interface_type: String,
    pub media_type: String,
    pub size: u64, // Bytes
    pub serial_number: String,
    pub firmware_revision: String,
    pub partitions: u32,
}

/// 显卡信息
#[derive(Debug, Clone, Default)]
pub struct GpuInfo {
    pub name: String,
    pub adapter_compatibility: String,
    pub driver_version: String,
    pub driver_date: String,
    pub video_memory: u64, // Bytes
    pub current_resolution: String,
    pub refresh_rate: u32, // Hz
    pub video_processor: String,
}

/// 操作系统信息
#[derive(Debug, Clone, Default)]
pub struct OsInfo {
    pub name: String,
    pub version: String,
    pub build_number: String,
    pub architecture: String,
    pub product_id: String,
    pub registered_owner: String,
    pub install_date: String,
}

/// 完整硬件信息
#[derive(Debug, Clone, Default)]
pub struct HardwareInfo {
    pub cpu: CpuInfo,
    pub memory: MemoryInfo,
    pub motherboard: MotherboardInfo,
    pub bios: BiosInfo,
    pub disks: Vec<DiskInfo>,
    pub gpus: Vec<GpuInfo>,
    pub os: OsInfo,
    pub computer_name: String,
    pub computer_model: String,
    pub computer_manufacturer: String,
}

// STORAGE_DEVICE_DESCRIPTOR 结构体
#[repr(C)]
#[allow(non_snake_case, dead_code)]
struct STORAGE_DEVICE_DESCRIPTOR {
    Version: u32,
    Size: u32,
    DeviceType: u8,
    DeviceTypeModifier: u8,
    RemovableMedia: u8,
    CommandQueueing: u8,
    VendorIdOffset: u32,
    ProductIdOffset: u32,
    ProductRevisionOffset: u32,
    SerialNumberOffset: u32,
    BusType: u32,
    RawPropertiesLength: u32,
    RawDeviceProperties: [u8; 1],
}

// GET_LENGTH_INFORMATION 结构体
#[repr(C)]
#[allow(non_snake_case, dead_code)]
struct GET_LENGTH_INFORMATION {
    length: i64,
}

impl HardwareInfo {
    /// 收集所有硬件信息
    pub fn collect() -> Result<Self, Box<dyn std::error::Error>> {
        let mut info = HardwareInfo::default();

        // 获取计算机基本信息（从注册表）
        Self::get_computer_info(&mut info);

        // 获取操作系统信息
        info.os = Self::get_os_info();

        // 获取 CPU 信息
        info.cpu = Self::get_cpu_info();

        // 获取内存信息
        info.memory = Self::get_memory_info();

        // 获取主板信息
        info.motherboard = Self::get_motherboard_info();

        // 获取 BIOS 信息
        info.bios = Self::get_bios_info();

        // 获取硬盘信息
        info.disks = Self::get_disk_info();

        // 获取显卡信息
        info.gpus = Self::get_gpu_info();

        Ok(info)
    }

    /// 从注册表获取计算机基本信息
    fn get_computer_info(info: &mut HardwareInfo) {
        // 获取计算机名
        if let Some(name) = read_registry_string(
            HKEY_LOCAL_MACHINE,
            r"SYSTEM\CurrentControlSet\Control\ComputerName\ComputerName",
            "ComputerName",
        ) {
            info.computer_name = name;
        }

        // 从 BIOS 注册表获取系统制造商和型号
        if let Some(manufacturer) = read_registry_string(
            HKEY_LOCAL_MACHINE,
            r"HARDWARE\DESCRIPTION\System\BIOS",
            "SystemManufacturer",
        ) {
            info.computer_manufacturer = manufacturer;
        }

        if let Some(model) = read_registry_string(
            HKEY_LOCAL_MACHINE,
            r"HARDWARE\DESCRIPTION\System\BIOS",
            "SystemProductName",
        ) {
            info.computer_model = model;
        }
    }

    /// 获取操作系统信息
    fn get_os_info() -> OsInfo {
        let mut os_info = OsInfo::default();

        let nt_path = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";

        // 先获取构建号，用于后续判断系统版本
        let build_number: u32 = read_registry_string(HKEY_LOCAL_MACHINE, nt_path, "CurrentBuild")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        // 获取产品名称
        if let Some(name) = read_registry_string(HKEY_LOCAL_MACHINE, nt_path, "ProductName") {
            // 根据构建号修正系统名称
            // Windows 11: 构建号 >= 22000
            // Windows 10: 构建号 10240 - 21999
            // Windows 8.1: 构建号 9600
            // Windows 8: 构建号 9200
            // Windows 7: 构建号 7600/7601
            if build_number >= 22000 && name.contains("Windows 10") {
                // Windows 11 但注册表显示 Windows 10，需要修正
                os_info.name = name.replace("Windows 10", "Windows 11");
            } else {
                os_info.name = name;
            }
        } else {
            // 如果无法读取 ProductName，根据构建号推断
            os_info.name = if build_number >= 22000 {
                "Windows 11".to_string()
            } else if build_number >= 10240 {
                "Windows 10".to_string()
            } else if build_number >= 9600 {
                "Windows 8.1".to_string()
            } else if build_number >= 9200 {
                "Windows 8".to_string()
            } else if build_number >= 7600 {
                "Windows 7".to_string()
            } else {
                "Windows".to_string()
            };
        }

        // 获取显示版本 (如 22H2, 23H2, 24H2)
        if let Some(display_version) = read_registry_string(HKEY_LOCAL_MACHINE, nt_path, "DisplayVersion") {
            os_info.version = display_version;
        } else if let Some(release_id) = read_registry_string(HKEY_LOCAL_MACHINE, nt_path, "ReleaseId") {
            os_info.version = release_id;
        }

        // 获取完整构建号 (如 22631.4890)
        if build_number > 0 {
            let ubr = read_registry_dword(HKEY_LOCAL_MACHINE, nt_path, "UBR")
                .map(|u| format!(".{}", u))
                .unwrap_or_default();
            os_info.build_number = format!("{}{}", build_number, ubr);
        }

        // 获取架构
        unsafe {
            let mut sys_info: SYSTEM_INFO = zeroed();
            GetNativeSystemInfo(&mut sys_info);
            os_info.architecture = match sys_info.Anonymous.Anonymous.wProcessorArchitecture.0 {
                0 => "32 位".to_string(),
                9 => "64 位".to_string(),
                12 => "ARM64".to_string(),
                _ => "未知".to_string(),
            };
        }

        // 获取产品 ID
        if let Some(product_id) = read_registry_string(HKEY_LOCAL_MACHINE, nt_path, "ProductId") {
            os_info.product_id = product_id;
        }

        // 获取注册用户
        if let Some(owner) = read_registry_string(HKEY_LOCAL_MACHINE, nt_path, "RegisteredOwner") {
            os_info.registered_owner = owner;
        }

        // 获取安装日期
        if let Some(install_date) = read_registry_dword(HKEY_LOCAL_MACHINE, nt_path, "InstallDate") {
            // Unix 时间戳转换为日期
            let datetime = chrono::DateTime::from_timestamp(install_date as i64, 0);
            if let Some(dt) = datetime {
                os_info.install_date = dt.format("%Y-%m-%d").to_string();
            }
        }

        os_info
    }

    /// 获取 CPU 信息
    fn get_cpu_info() -> CpuInfo {
        let mut cpu_info = CpuInfo::default();

        // 使用 GetNativeSystemInfo 获取基本信息
        unsafe {
            let mut sys_info: SYSTEM_INFO = zeroed();
            GetNativeSystemInfo(&mut sys_info);

            cpu_info.logical_processors = sys_info.dwNumberOfProcessors;

            // 处理器架构
            cpu_info.architecture = match sys_info.Anonymous.Anonymous.wProcessorArchitecture.0 {
                0 => "x86".to_string(),
                5 => "ARM".to_string(),
                6 => "IA-64".to_string(),
                9 => "x64".to_string(),
                12 => "ARM64".to_string(),
                _ => "未知".to_string(),
            };
        }

        // 从注册表获取详细信息
        let cpu_path = r"HARDWARE\DESCRIPTION\System\CentralProcessor\0";

        if let Some(name) = read_registry_string(HKEY_LOCAL_MACHINE, cpu_path, "ProcessorNameString") {
            cpu_info.name = name.trim().to_string();
        }

        if let Some(vendor) = read_registry_string(HKEY_LOCAL_MACHINE, cpu_path, "VendorIdentifier") {
            cpu_info.manufacturer = vendor;
        }

        if let Some(mhz) = read_registry_dword(HKEY_LOCAL_MACHINE, cpu_path, "~MHz") {
            cpu_info.max_clock_speed = mhz;
            cpu_info.current_clock_speed = mhz;
        }

        // 尝试获取核心数（遍历处理器）
        let mut core_count = 0u32;
        for i in 0..256 {
            let path = format!(r"HARDWARE\DESCRIPTION\System\CentralProcessor\{}", i);
            if read_registry_string(HKEY_LOCAL_MACHINE, &path, "ProcessorNameString").is_some() {
                core_count += 1;
            } else {
                break;
            }
        }
        if core_count > 0 {
            cpu_info.cores = core_count;
        }

        cpu_info
    }

    /// 获取内存信息
    fn get_memory_info() -> MemoryInfo {
        let mut mem_info = MemoryInfo::default();

        unsafe {
            let mut mem_status: MEMORYSTATUSEX = zeroed();
            mem_status.dwLength = size_of::<MEMORYSTATUSEX>() as u32;

            if GlobalMemoryStatusEx(&mut mem_status).is_ok() {
                mem_info.total_physical = mem_status.ullTotalPhys;
                mem_info.available_physical = mem_status.ullAvailPhys;
                mem_info.total_virtual = mem_status.ullTotalVirtual;
                mem_info.available_virtual = mem_status.ullAvailVirtual;
                mem_info.memory_load = mem_status.dwMemoryLoad;
            }
        }

        mem_info
    }

    /// 获取主板信息
    fn get_motherboard_info() -> MotherboardInfo {
        let mut mb_info = MotherboardInfo::default();

        let bios_path = r"HARDWARE\DESCRIPTION\System\BIOS";

        if let Some(manufacturer) = read_registry_string(HKEY_LOCAL_MACHINE, bios_path, "BaseBoardManufacturer") {
            mb_info.manufacturer = manufacturer;
        }

        if let Some(product) = read_registry_string(HKEY_LOCAL_MACHINE, bios_path, "BaseBoardProduct") {
            mb_info.product = product;
        }

        if let Some(version) = read_registry_string(HKEY_LOCAL_MACHINE, bios_path, "BaseBoardVersion") {
            mb_info.version = version;
        }

        mb_info
    }

    /// 获取 BIOS 信息
    fn get_bios_info() -> BiosInfo {
        let mut bios_info = BiosInfo::default();

        let bios_path = r"HARDWARE\DESCRIPTION\System\BIOS";

        if let Some(vendor) = read_registry_string(HKEY_LOCAL_MACHINE, bios_path, "BIOSVendor") {
            bios_info.manufacturer = vendor;
        }

        if let Some(version) = read_registry_string(HKEY_LOCAL_MACHINE, bios_path, "BIOSVersion") {
            bios_info.version = version;
        }

        if let Some(date) = read_registry_string(HKEY_LOCAL_MACHINE, bios_path, "BIOSReleaseDate") {
            bios_info.release_date = date;
        }

        if let Some(smbios) = read_registry_string(HKEY_LOCAL_MACHINE, bios_path, "SystemBiosVersion") {
            bios_info.smbios_version = smbios;
        }

        bios_info
    }

    /// 获取硬盘信息
    fn get_disk_info() -> Vec<DiskInfo> {
        let mut disks = Vec::new();

        // 遍历物理驱动器 0-15
        for i in 0..16 {
            let path = format!(r"\\.\PhysicalDrive{}", i);
            if let Some(disk) = query_disk_info(&path) {
                disks.push(disk);
            }
        }

        disks
    }

    /// 获取显卡信息
    fn get_gpu_info() -> Vec<GpuInfo> {
        let mut gpus = Vec::new();

        unsafe {
            let mut device: DISPLAY_DEVICEW = zeroed();
            device.cb = size_of::<DISPLAY_DEVICEW>() as u32;

            let mut index = 0u32;
            // EnumDisplayDevicesW 返回 BOOL
            while EnumDisplayDevicesW(PCWSTR::null(), index, &mut device, 0) != BOOL(0) {
                // StateFlags 是 u32，DISPLAY_DEVICE_ACTIVE 值是 1
                const DISPLAY_DEVICE_ACTIVE_FLAG: u32 = 1;
                if (device.StateFlags & DISPLAY_DEVICE_ACTIVE_FLAG) != 0 {
                    let device_string = wchar_to_string(&device.DeviceString);
                    
                    // 跳过远程桌面等虚拟设备
                    if !device_string.contains("Remote") && !device_string.is_empty() {
                        let mut gpu = GpuInfo::default();
                        gpu.name = device_string.trim().to_string();

                        // 获取显示模式信息
                        if let Some((resolution, refresh)) = get_display_mode(&device.DeviceName) {
                            gpu.current_resolution = resolution;
                            gpu.refresh_rate = refresh;
                        }

                        gpus.push(gpu);
                    }
                }
                
                index += 1;
                device = zeroed();
                device.cb = size_of::<DISPLAY_DEVICEW>() as u32;
            }
        }

        gpus
    }
}

/// 从注册表读取字符串值
fn read_registry_string(hkey: HKEY, subkey: &str, value_name: &str) -> Option<String> {
    unsafe {
        let subkey_wide: Vec<u16> = subkey.encode_utf16().chain(std::iter::once(0)).collect();
        let value_name_wide: Vec<u16> = value_name.encode_utf16().chain(std::iter::once(0)).collect();

        let mut key_handle: HKEY = HKEY::default();
        let result = RegOpenKeyExW(
            hkey,
            PCWSTR(subkey_wide.as_ptr()),
            0,
            KEY_READ,
            &mut key_handle,
        );

        if result.is_err() {
            return None;
        }

        let mut buffer: Vec<u8> = vec![0u8; 1024];
        let mut buffer_size = buffer.len() as u32;
        let mut value_type: REG_VALUE_TYPE = REG_VALUE_TYPE(0);

        let result = RegQueryValueExW(
            key_handle,
            PCWSTR(value_name_wide.as_ptr()),
            None,
            Some(&mut value_type),
            Some(buffer.as_mut_ptr()),
            Some(&mut buffer_size),
        );

        let _ = RegCloseKey(key_handle);

        if result.is_err() {
            return None;
        }

        // REG_SZ 的值是 1
        if value_type.0 != 1 {
            return None;
        }

        // 转换为字符串 (UTF-16)
        let len = (buffer_size as usize) / 2;
        if len > 0 {
            let wide: Vec<u16> = buffer[..len * 2]
                .chunks(2)
                .map(|c| u16::from_le_bytes([c[0], c.get(1).copied().unwrap_or(0)]))
                .collect();
            let s = OsString::from_wide(&wide[..wide.len().saturating_sub(1)]);
            return Some(s.to_string_lossy().to_string());
        }

        None
    }
}

/// 从注册表读取 DWORD 值
fn read_registry_dword(hkey: HKEY, subkey: &str, value_name: &str) -> Option<u32> {
    unsafe {
        let subkey_wide: Vec<u16> = subkey.encode_utf16().chain(std::iter::once(0)).collect();
        let value_name_wide: Vec<u16> = value_name.encode_utf16().chain(std::iter::once(0)).collect();

        let mut key_handle: HKEY = HKEY::default();
        let result = RegOpenKeyExW(
            hkey,
            PCWSTR(subkey_wide.as_ptr()),
            0,
            KEY_READ,
            &mut key_handle,
        );

        if result.is_err() {
            return None;
        }

        let mut value: u32 = 0;
        let mut buffer_size = size_of::<u32>() as u32;
        let mut value_type: REG_VALUE_TYPE = REG_VALUE_TYPE(0);

        let result = RegQueryValueExW(
            key_handle,
            PCWSTR(value_name_wide.as_ptr()),
            None,
            Some(&mut value_type),
            Some(&mut value as *mut u32 as *mut u8),
            Some(&mut buffer_size),
        );

        let _ = RegCloseKey(key_handle);

        if result.is_err() {
            return None;
        }

        // REG_DWORD 的值是 4
        if value_type.0 != 4 {
            return None;
        }

        Some(value)
    }
}

/// 将宽字符数组转换为字符串
fn wchar_to_string(wchars: &[u16]) -> String {
    let len = wchars.iter().position(|&c| c == 0).unwrap_or(wchars.len());
    OsString::from_wide(&wchars[..len]).to_string_lossy().to_string()
}

/// 查询单个硬盘信息
fn query_disk_info(path: &str) -> Option<DiskInfo> {
    unsafe {
        let path_wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
        
        let handle = CreateFileW(
            PCWSTR(path_wide.as_ptr()),
            0, // 只需要查询，不需要读写权限
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_FLAGS_AND_ATTRIBUTES(0),
            HANDLE::default(),
        );

        let handle = match handle {
            Ok(h) if h != INVALID_HANDLE_VALUE => h,
            _ => return None,
        };

        // 准备查询
        let mut query: STORAGE_PROPERTY_QUERY = zeroed();
        query.PropertyId = StorageDeviceProperty;
        query.QueryType = PropertyStandardQuery;

        let mut buffer = vec![0u8; 4096];
        let mut bytes_returned: u32 = 0;

        let result = DeviceIoControl(
            handle,
            IOCTL_STORAGE_QUERY_PROPERTY,
            Some(&query as *const _ as *const std::ffi::c_void),
            size_of::<STORAGE_PROPERTY_QUERY>() as u32,
            Some(buffer.as_mut_ptr() as *mut std::ffi::c_void),
            buffer.len() as u32,
            Some(&mut bytes_returned),
            None,
        );

        if result.is_err() || bytes_returned == 0 {
            let _ = CloseHandle(handle);
            return None;
        }

        // 解析 STORAGE_DEVICE_DESCRIPTOR
        let descriptor = &*(buffer.as_ptr() as *const STORAGE_DEVICE_DESCRIPTOR);
        
        let mut disk = DiskInfo::default();

        // 获取产品 ID（型号）
        if descriptor.ProductIdOffset > 0 && (descriptor.ProductIdOffset as usize) < buffer.len() {
            let offset = descriptor.ProductIdOffset as usize;
            if let Some(end) = buffer[offset..].iter().position(|&b| b == 0) {
                disk.model = String::from_utf8_lossy(&buffer[offset..offset + end]).trim().to_string();
            }
        }

        // 获取序列号
        if descriptor.SerialNumberOffset > 0 && (descriptor.SerialNumberOffset as usize) < buffer.len() {
            let offset = descriptor.SerialNumberOffset as usize;
            if let Some(end) = buffer[offset..].iter().position(|&b| b == 0) {
                disk.serial_number = String::from_utf8_lossy(&buffer[offset..offset + end]).trim().to_string();
            }
        }

        // 获取固件版本
        if descriptor.ProductRevisionOffset > 0 && (descriptor.ProductRevisionOffset as usize) < buffer.len() {
            let offset = descriptor.ProductRevisionOffset as usize;
            if let Some(end) = buffer[offset..].iter().position(|&b| b == 0) {
                disk.firmware_revision = String::from_utf8_lossy(&buffer[offset..offset + end]).trim().to_string();
            }
        }

        // 总线类型
        disk.interface_type = match descriptor.BusType {
            1 => "SCSI".to_string(),
            2 => "ATAPI".to_string(),
            3 => "ATA".to_string(),
            4 => "1394".to_string(),
            5 => "SSA".to_string(),
            6 => "Fibre".to_string(),
            7 => "USB".to_string(),
            8 => "RAID".to_string(),
            9 => "iSCSI".to_string(),
            10 => "SAS".to_string(),
            11 => "SATA".to_string(),
            12 => "SD".to_string(),
            13 => "MMC".to_string(),
            14 => "Virtual".to_string(),
            15 => "FileBackedVirtual".to_string(),
            17 => "NVMe".to_string(),
            _ => format!("Unknown({})", descriptor.BusType),
        };

        // 介质类型
        if descriptor.RemovableMedia != 0 {
            disk.media_type = "可移动".to_string();
        } else {
            disk.media_type = "固定".to_string();
        }

        // 获取硬盘大小
        let mut length_info: GET_LENGTH_INFORMATION = zeroed();
        let mut bytes_ret: u32 = 0;
        
        if DeviceIoControl(
            handle,
            IOCTL_DISK_GET_LENGTH_INFO,
            None,
            0,
            Some(&mut length_info as *mut _ as *mut std::ffi::c_void),
            size_of::<GET_LENGTH_INFORMATION>() as u32,
            Some(&mut bytes_ret),
            None,
        ).is_ok() {
            disk.size = length_info.length as u64;
        }

        let _ = CloseHandle(handle);

        // 只返回有效的磁盘
        if !disk.model.is_empty() || disk.size > 0 {
            Some(disk)
        } else {
            None
        }
    }
}

/// 获取显示模式
fn get_display_mode(device_name: &[u16]) -> Option<(String, u32)> {
    unsafe {
        let mut devmode: DEVMODEW = zeroed();
        devmode.dmSize = size_of::<DEVMODEW>() as u16;

        if EnumDisplaySettingsW(
            PCWSTR(device_name.as_ptr()),
            ENUM_CURRENT_SETTINGS,
            &mut devmode,
        ) != BOOL(0) {
            let resolution = format!("{}x{}", devmode.dmPelsWidth, devmode.dmPelsHeight);
            let refresh = devmode.dmDisplayFrequency;
            return Some((resolution, refresh));
        }

        None
    }
}

/// 格式化字节大小
pub fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    const TB: u64 = GB * 1024;

    if bytes >= TB {
        format!("{:.2} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} Bytes", bytes)
    }
}
