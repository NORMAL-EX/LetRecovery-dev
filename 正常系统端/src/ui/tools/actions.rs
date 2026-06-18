//! 工具操作模块
//!
//! 提供各种工具的启动和操作功能

use std::process::Command;
use crate::utils::path::{get_bin_dir, get_tools_dir};

/// 启动指定工具
pub fn launch_tool(tool_name: &str) -> Result<(), String> {
    let tools_dir = get_tools_dir();
    let tool_path = tools_dir.join(tool_name);

    if tool_path.exists() {
        let result = if tool_name.to_lowercase().ends_with(".cpl") {
            Command::new("control.exe").arg(&tool_path).spawn()
        } else {
            Command::new(&tool_path).spawn()
        };

        match result {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("启动失败: {} - {}", tool_name, e)),
        }
    } else {
        Err(format!("工具不存在: {:?}", tool_path))
    }
}

/// 启动Ghost工具
pub fn launch_ghost() -> Result<(), String> {
    let bin_dir = get_bin_dir();
    let ghost_path = bin_dir.join("ghost").join("Ghost64.exe");

    if ghost_path.exists() {
        match Command::new(&ghost_path).spawn() {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("启动失败: Ghost64.exe - {}", e)),
        }
    } else {
        Err(format!("工具不存在: {:?}", ghost_path))
    }
}

/// 启动 SpaceSniffer 磁盘空间分析工具
pub fn launch_space_sniffer() -> Result<(), String> {
    let tools_dir = get_tools_dir();
    let space_sniffer_path = tools_dir.join("SpaceSniffer.exe");

    if space_sniffer_path.exists() {
        match Command::new(&space_sniffer_path).spawn() {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("启动失败: SpaceSniffer.exe - {}", e)),
        }
    } else {
        Err(format!("工具不存在: {:?}", space_sniffer_path))
    }
}

/// 修复引导
pub fn repair_boot(target_partition: &str) -> Result<(), String> {
    let boot_manager = crate::core::bcdedit::BootManager::new();
    boot_manager.repair_boot(target_partition)
        .map_err(|e| e.to_string())
}

/// 导出当前系统驱动
pub fn export_drivers(export_dir: &str) -> Result<(), String> {
    let dism = crate::core::dism::Dism::new();
    dism.export_drivers(export_dir)
        .map_err(|e| e.to_string())
}

/// 从指定分区导出驱动
pub fn export_drivers_from_partition(source_partition: &str, export_dir: &str) -> Result<(), String> {
    let dism = crate::core::dism::Dism::new();
    dism.export_drivers_from_system(source_partition, export_dir)
        .map_err(|e| e.to_string())
}

/// 运行 Diskpart 脚本：写入临时脚本文件后执行 `diskpart /s`，返回合并后的输出文本。
///
/// 失败时返回 Err（含 diskpart 的输出，便于排查）。脚本里的命令按行执行，与
/// 在 diskpart 交互式里逐行输入等价。
pub fn run_diskpart_script(script: &str) -> Result<String, String> {
    use std::io::Write;

    if script.trim().is_empty() {
        return Err("脚本内容为空".to_string());
    }

    // 统一为 CRLF 行尾，确保 diskpart 正确逐行解析。
    let normalized = script
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\n', "\r\n");

    let script_path =
        std::env::temp_dir().join(format!("lr_diskpart_{}.txt", std::process::id()));
    {
        let mut f = std::fs::File::create(&script_path)
            .map_err(|e| format!("创建临时脚本失败: {}", e))?;
        f.write_all(normalized.as_bytes())
            .map_err(|e| format!("写入临时脚本失败: {}", e))?;
    }

    let result = crate::utils::cmd::create_command("diskpart")
        .args(["/s", &script_path.to_string_lossy()])
        .output();

    let _ = std::fs::remove_file(&script_path);

    match result {
        Ok(out) => {
            let mut text = crate::utils::encoding::gbk_to_utf8(&out.stdout);
            let err_text = crate::utils::encoding::gbk_to_utf8(&out.stderr);
            if !err_text.trim().is_empty() {
                text.push('\n');
                text.push_str(&err_text);
            }
            let text = text.trim().to_string();
            if out.status.success() {
                Ok(text)
            } else {
                Err(format!("diskpart 返回错误。\n{}", text))
            }
        }
        Err(e) => Err(format!("无法启动 diskpart: {}", e)),
    }
}
