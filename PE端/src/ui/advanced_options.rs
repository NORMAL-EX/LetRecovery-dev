use walkdir::WalkDir;

use crate::core::config::InstallConfig;
use crate::core::registry::OfflineRegistry;

/// 应用高级选项到目标系统
pub fn apply_advanced_options(target_partition: &str, config: &InstallConfig) -> anyhow::Result<()> {
    let windows_path = format!("{}\\Windows", target_partition);
    let software_hive = format!("{}\\System32\\config\\SOFTWARE", windows_path);
    let system_hive = format!("{}\\System32\\config\\SYSTEM", windows_path);

    log::info!("应用高级选项到 {}", target_partition);

    // 加载离线注册表
    OfflineRegistry::load_hive("pc-soft", &software_hive)?;
    OfflineRegistry::load_hive("pc-sys", &system_hive)?;

    // 创建自定义目录
    let custom_dir = format!("{}\\letzdy", target_partition);
    std::fs::create_dir_all(&custom_dir)?;

    // 移除快捷方式小箭头
    if config.remove_shortcut_arrow {
        log::info!("移除快捷方式小箭头");
        let _ = OfflineRegistry::set_string(
            "HKLM\\pc-soft\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Shell Icons",
            "29",
            "%systemroot%\\system32\\imageres.dll,197",
        );
    }

    // Win11恢复经典右键
    if config.restore_classic_context_menu {
        log::info!("恢复经典右键菜单");
        std::fs::write(format!("{}\\bas", custom_dir), "1")?;
    }

    // OOBE绕过强制联网
    if config.bypass_nro {
        log::info!("绕过OOBE强制联网");
        let _ = OfflineRegistry::set_dword(
            "HKLM\\pc-soft\\Microsoft\\Windows\\CurrentVersion\\OOBE",
            "BypassNRO",
            1,
        );
    }

    // 禁用Windows安全中心
    if config.disable_windows_defender {
        log::info!("禁用Windows安全中心");
        let _ = OfflineRegistry::delete_key("HKLM\\pc-sys\\ControlSet001\\Services\\WinDefend");
        let _ = OfflineRegistry::delete_key("HKLM\\pc-sys\\ControlSet001\\Services\\WdNisSvc");
    }

    // 禁用系统保留空间
    if config.disable_reserved_storage {
        log::info!("禁用系统保留空间");
        std::fs::write(format!("{}\\yl", custom_dir), "1")?;
    }

    // 禁用UAC
    if config.disable_uac {
        log::info!("禁用UAC");
        let _ = OfflineRegistry::set_dword(
            "HKLM\\pc-soft\\Microsoft\\Windows\\CurrentVersion\\Policies\\System",
            "EnableLUA",
            0,
        );
    }

    // 禁用自动设备加密
    if config.disable_device_encryption {
        log::info!("禁用自动设备加密");
        std::fs::write(format!("{}\\nobl", custom_dir), "1")?;
    }

    // 删除预装UWP
    if config.remove_uwp_apps {
        log::info!("标记删除预装UWP");
        std::fs::write(format!("{}\\nuwp", custom_dir), "1")?;
    }

    // 禁用Windows更新
    if config.disable_windows_update {
        log::info!("禁用Windows更新");
        let _ = OfflineRegistry::delete_key("HKLM\\pc-sys\\ControlSet001\\Services\\wuauserv");
    }

    // 自定义用户名
    if !config.custom_username.is_empty() {
        log::info!("设置自定义用户名: {}", config.custom_username);
        std::fs::write(format!("{}\\zdyusername.let", custom_dir), &config.custom_username)?;
    }

    // 卸载注册表（确保正确卸载）
    std::thread::sleep(std::time::Duration::from_millis(500));
    let _ = OfflineRegistry::unload_hive("pc-soft");
    let _ = OfflineRegistry::unload_hive("pc-sys");

    log::info!("高级选项应用完成");
    Ok(())
}

/// 复制目录（递归）
pub fn copy_dir_all(src: &str, dst: &str) -> anyhow::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in WalkDir::new(src) {
        let entry = entry?;
        let target = std::path::Path::new(dst).join(entry.path().strip_prefix(src)?);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&target)?;
        } else {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}
