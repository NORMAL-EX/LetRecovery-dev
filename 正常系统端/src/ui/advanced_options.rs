use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use crate::core::registry::OfflineRegistry;

/// 系统安装高级选项
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AdvancedOptions {
    // 系统优化选项
    pub remove_shortcut_arrow: bool,
    pub restore_classic_context_menu: bool,
    pub bypass_nro: bool,
    pub disable_windows_update: bool,
    pub disable_windows_defender: bool,
    pub disable_reserved_storage: bool,
    pub disable_uac: bool,
    pub disable_device_encryption: bool,
    pub remove_uwp_apps: bool,

    // 自定义脚本
    pub run_script_during_deploy: bool,
    pub deploy_script_path: String,
    pub run_script_first_login: bool,
    pub first_login_script_path: String,

    // 自定义内容
    pub import_custom_drivers: bool,
    pub custom_drivers_path: String,
    pub import_registry_file: bool,
    pub registry_file_path: String,
    pub import_custom_files: bool,
    pub custom_files_path: String,

    // 用户设置
    pub custom_username: bool,
    pub username: String,
}

impl AdvancedOptions {
    /// 应用选项到目标系统
    pub fn apply_to_system(&self, target_partition: &str) -> anyhow::Result<()> {
        let windows_path = format!("{}\\Windows", target_partition);
        let software_hive = format!("{}\\System32\\config\\SOFTWARE", windows_path);
        let system_hive = format!("{}\\System32\\config\\SYSTEM", windows_path);

        // 加载离线注册表
        OfflineRegistry::load_hive("pc-soft", &software_hive)?;
        OfflineRegistry::load_hive("pc-sys", &system_hive)?;

        // 创建自定义目录
        let custom_dir = format!("{}\\letzdy", target_partition);
        std::fs::create_dir_all(&custom_dir)?;

        // 移除快捷方式小箭头
        if self.remove_shortcut_arrow {
            let _ = OfflineRegistry::set_string(
                "HKLM\\pc-soft\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Shell Icons",
                "29",
                "%systemroot%\\system32\\imageres.dll,197",
            );
        }

        // Win11恢复经典右键
        if self.restore_classic_context_menu {
            std::fs::write(format!("{}\\bas", custom_dir), "1")?;
        }

        // OOBE绕过强制联网
        if self.bypass_nro {
            let _ = OfflineRegistry::set_dword(
                "HKLM\\pc-soft\\Microsoft\\Windows\\CurrentVersion\\OOBE",
                "BypassNRO",
                1,
            );
        }

        // 禁用Windows安全中心
        if self.disable_windows_defender {
            let _ = OfflineRegistry::delete_key("HKLM\\pc-sys\\ControlSet001\\Services\\WinDefend");
            let _ = OfflineRegistry::delete_key("HKLM\\pc-sys\\ControlSet001\\Services\\WdNisSvc");
        }

        // 禁用系统保留空间
        if self.disable_reserved_storage {
            std::fs::write(format!("{}\\yl", custom_dir), "1")?;
        }

        // 禁用UAC
        if self.disable_uac {
            let _ = OfflineRegistry::set_dword(
                "HKLM\\pc-soft\\Microsoft\\Windows\\CurrentVersion\\Policies\\System",
                "EnableLUA",
                0,
            );
        }

        // 禁用自动设备加密
        if self.disable_device_encryption {
            std::fs::write(format!("{}\\nobl", custom_dir), "1")?;
        }

        // 删除预装UWP
        if self.remove_uwp_apps {
            std::fs::write(format!("{}\\nuwp", custom_dir), "1")?;
        }

        // 禁用Windows更新
        if self.disable_windows_update {
            let _ = OfflineRegistry::delete_key("HKLM\\pc-sys\\ControlSet001\\Services\\wuauserv");
        }

        // 复制自定义脚本
        if self.run_script_during_deploy && !self.deploy_script_path.is_empty() {
            let _ = std::fs::copy(&self.deploy_script_path, format!("{}\\zdy1.bat", custom_dir));
        }

        if self.run_script_first_login && !self.first_login_script_path.is_empty() {
            let _ = std::fs::copy(
                &self.first_login_script_path,
                format!("{}\\zdy2.bat", custom_dir),
            );
        }

        // 复制注册表文件
        if self.import_registry_file && !self.registry_file_path.is_empty() {
            let _ = std::fs::copy(&self.registry_file_path, format!("{}\\zdy.reg", custom_dir));
        }

        // 复制自定义驱动
        if self.import_custom_drivers && !self.custom_drivers_path.is_empty() {
            let target_driver_dir = format!("{}\\zdydri", target_partition);
            // 先删除旧目录，避免残留文件冲突
            let _ = std::fs::remove_dir_all(&target_driver_dir);
            let _ = Self::copy_dir_all(&self.custom_drivers_path, &target_driver_dir);
        }

        // 复制自定义文件
        if self.import_custom_files && !self.custom_files_path.is_empty() {
            let _ = Self::copy_dir_all(&self.custom_files_path, target_partition);
        }

        // 自定义用户名
        if self.custom_username && !self.username.is_empty() {
            std::fs::write(format!("{}\\zdyusername.let", custom_dir), &self.username)?;
        }

        // 卸载注册表
        let _ = OfflineRegistry::unload_hive("pc-soft");
        let _ = OfflineRegistry::unload_hive("pc-sys");

        Ok(())
    }

    fn copy_dir_all(src: &str, dst: &str) -> anyhow::Result<()> {
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

    /// 显示高级选项界面
    pub fn show_ui(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.heading("系统优化选项");
            ui.separator();

            ui.checkbox(&mut self.remove_shortcut_arrow, "移除快捷方式小箭头");
            ui.checkbox(&mut self.restore_classic_context_menu, "Win11恢复经典右键菜单");
            ui.checkbox(&mut self.bypass_nro, "OOBE绕过强制联网");
            ui.checkbox(&mut self.disable_windows_update, "禁用Windows更新");
            ui.checkbox(&mut self.disable_windows_defender, "禁用Windows安全中心");
            ui.checkbox(&mut self.disable_reserved_storage, "禁用系统保留空间");
            ui.checkbox(&mut self.disable_uac, "禁用用户账户控制(UAC)");
            ui.checkbox(&mut self.disable_device_encryption, "禁用自动设备加密");
            ui.checkbox(&mut self.remove_uwp_apps, "删除预装UWP应用");

            ui.add_space(15.0);
            ui.heading("自定义脚本");
            ui.separator();

            ui.horizontal(|ui| {
                ui.checkbox(&mut self.run_script_during_deploy, "系统部署中运行脚本");
                if self.run_script_during_deploy {
                    ui.text_edit_singleline(&mut self.deploy_script_path);
                    if ui.button("浏览...").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("批处理文件", &["bat", "cmd"])
                            .pick_file()
                        {
                            self.deploy_script_path = path.to_string_lossy().to_string();
                        }
                    }
                }
            });

            ui.horizontal(|ui| {
                ui.checkbox(&mut self.run_script_first_login, "首次登录运行脚本");
                if self.run_script_first_login {
                    ui.text_edit_singleline(&mut self.first_login_script_path);
                    if ui.button("浏览...").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("批处理文件", &["bat", "cmd"])
                            .pick_file()
                        {
                            self.first_login_script_path = path.to_string_lossy().to_string();
                        }
                    }
                }
            });

            ui.add_space(15.0);
            ui.heading("自定义内容");
            ui.separator();

            ui.horizontal(|ui| {
                ui.checkbox(&mut self.import_custom_drivers, "导入自定义驱动");
                if self.import_custom_drivers {
                    ui.text_edit_singleline(&mut self.custom_drivers_path);
                    if ui.button("浏览...").clicked() {
                        if let Some(path) = rfd::FileDialog::new().pick_folder() {
                            self.custom_drivers_path = path.to_string_lossy().to_string();
                        }
                    }
                }
            });

            ui.horizontal(|ui| {
                ui.checkbox(&mut self.import_registry_file, "导入注册表文件");
                if self.import_registry_file {
                    ui.text_edit_singleline(&mut self.registry_file_path);
                    if ui.button("浏览...").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("注册表文件", &["reg"])
                            .pick_file()
                        {
                            self.registry_file_path = path.to_string_lossy().to_string();
                        }
                    }
                }
            });

            ui.horizontal(|ui| {
                ui.checkbox(&mut self.import_custom_files, "导入自定义文件");
                if self.import_custom_files {
                    ui.text_edit_singleline(&mut self.custom_files_path);
                    if ui.button("浏览...").clicked() {
                        if let Some(path) = rfd::FileDialog::new().pick_folder() {
                            self.custom_files_path = path.to_string_lossy().to_string();
                        }
                    }
                }
            });

            ui.add_space(15.0);
            ui.heading("用户设置");
            ui.separator();

            ui.horizontal(|ui| {
                ui.checkbox(&mut self.custom_username, "自定义用户名");
                if self.custom_username {
                    ui.text_edit_singleline(&mut self.username);
                }
            });
        });
    }
}

use egui;
