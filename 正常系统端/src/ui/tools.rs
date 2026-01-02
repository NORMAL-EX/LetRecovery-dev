use egui;
use std::process::Command;

use crate::app::App;
use crate::utils::cmd::create_command;
use crate::utils::path::get_tools_dir;

impl App {
    pub fn show_tools(&mut self, ui: &mut egui::Ui) {
        ui.heading("工具箱");
        ui.separator();

        let is_pe = self.system_info.as_ref().map(|s| s.is_pe_environment).unwrap_or(false);

        ui.label("常用工具");
        ui.add_space(10.0);

        egui::Grid::new("tools_grid")
            .num_columns(3)
            .spacing([20.0, 15.0])
            .show(ui, |ui| {
                // BOOTICE
                if ui
                    .add(egui::Button::new("BOOTICE\n引导修复工具").min_size(egui::vec2(120.0, 60.0)))
                    .clicked()
                {
                    self.launch_tool("BOOTICE.exe");
                }

                // 显示隐藏分区
                if ui
                    .add(
                        egui::Button::new("显示隐藏分区\nShowDrives").min_size(egui::vec2(120.0, 60.0)),
                    )
                    .clicked()
                {
                    self.launch_tool("ShowDrives_Gui.exe");
                }

                // 磁盘管理
                if ui
                    .add(
                        egui::Button::new("磁盘管理\ndiskmgmt.msc").min_size(egui::vec2(120.0, 60.0)),
                    )
                    .clicked()
                {
                    let _ = Command::new("mmc.exe")
                        .arg("diskmgmt.msc")
                        .spawn();
                }

                ui.end_row();

                // 设备管理器
                if ui
                    .add(
                        egui::Button::new("设备管理器\ndevmgmt.msc").min_size(egui::vec2(120.0, 60.0)),
                    )
                    .clicked()
                {
                    let _ = Command::new("mmc.exe")
                        .arg("devmgmt.msc")
                        .spawn();
                }

                // 命令提示符
                if ui
                    .add(egui::Button::new("命令提示符\ncmd.exe").min_size(egui::vec2(120.0, 60.0)))
                    .clicked()
                {
                    let _ = Command::new("cmd.exe").spawn();
                }

                // 资源管理器
                if ui
                    .add(
                        egui::Button::new("资源管理器\nexplorer.exe")
                            .min_size(egui::vec2(120.0, 60.0)),
                    )
                    .clicked()
                {
                    let _ = Command::new("explorer.exe").spawn();
                }

                ui.end_row();

                // 注册表编辑器
                if ui
                    .add(
                        egui::Button::new("注册表编辑器\nregedit.exe")
                            .min_size(egui::vec2(120.0, 60.0)),
                    )
                    .clicked()
                {
                    let _ = Command::new("regedit.exe").spawn();
                }

                // 任务管理器
                if ui
                    .add(
                        egui::Button::new("任务管理器\ntaskmgr.exe")
                            .min_size(egui::vec2(120.0, 60.0)),
                    )
                    .clicked()
                {
                    let _ = Command::new("taskmgr.exe").spawn();
                }

                // 记事本
                if ui
                    .add(egui::Button::new("记事本\nnotepad.exe").min_size(egui::vec2(120.0, 60.0)))
                    .clicked()
                {
                    let _ = Command::new("notepad.exe").spawn();
                }

                ui.end_row();

                // Ghost 工具
                if ui
                    .add(egui::Button::new("Ghost 工具\nGhost64.exe").min_size(egui::vec2(120.0, 60.0)))
                    .clicked()
                {
                    self.launch_ghost_tool();
                }

                // ImDisk 虚拟磁盘
                if ui
                    .add(egui::Button::new("ImDisk\n虚拟磁盘").min_size(egui::vec2(120.0, 60.0)))
                    .clicked()
                {
                    self.launch_tool("imdisk.cpl");
                }

                ui.end_row();
            });

        ui.add_space(20.0);
        ui.separator();

        ui.label("系统操作");
        ui.add_space(10.0);

        // PE 环境下显示分区选择
        if is_pe {
            // 筛选有系统的分区
            let system_partitions: Vec<_> = self.partitions.iter()
                .filter(|p| p.has_windows && p.letter.to_uppercase() != "X:")
                .collect();

            if system_partitions.is_empty() {
                // 没有找到有系统的分区，显示警告
                ui.colored_label(
                    egui::Color32::from_rgb(255, 165, 0),
                    "⚠ 未找到包含 Windows 系统的分区！"
                );
                ui.label("修复引导和导出驱动需要选择一个包含 Windows 系统的分区。");
                ui.add_space(5.0);
            } else {
                ui.horizontal(|ui| {
                    ui.label("目标系统分区:");
                    egui::ComboBox::from_id_salt("target_partition_tools")
                        .selected_text(
                            self.tool_target_partition
                                .as_ref()
                                .unwrap_or(&"请选择".to_string()),
                        )
                        .show_ui(ui, |ui| {
                            for partition in system_partitions {
                                let label = format!(
                                    "{} {} ({:.1} GB) [有系统]",
                                    partition.letter,
                                    partition.label,
                                    partition.total_size_mb as f64 / 1024.0
                                );
                                ui.selectable_value(
                                    &mut self.tool_target_partition,
                                    Some(partition.letter.clone()),
                                    label,
                                );
                            }
                        });
                });
                ui.add_space(5.0);
            }
        }

        ui.horizontal(|ui| {
            if ui.button("修复系统引导").clicked() {
                self.repair_boot_action(is_pe);
            }

            if ui.button("导出系统驱动").clicked() {
                self.export_drivers_action(is_pe);
            }
        });

        ui.add_space(10.0);

        ui.horizontal(|ui| {
            if ui.button("重启计算机").clicked() {
                let _ = create_command("shutdown")
                    .args(["/r", "/t", "0"])
                    .spawn();
            }

            if ui.button("关闭计算机").clicked() {
                let _ = create_command("shutdown")
                    .args(["/s", "/t", "0"])
                    .spawn();
            }
        });

        // 显示工具状态
        if !self.tool_message.is_empty() {
            ui.add_space(15.0);
            ui.separator();
            ui.label(&self.tool_message);
        }
    }

    fn launch_tool(&mut self, tool_name: &str) {
        let tools_dir = get_tools_dir();
        let tool_path = tools_dir.join(tool_name);

        if tool_path.exists() {
            // 检查文件扩展名，对.cpl文件使用特殊处理
            let result = if tool_name.to_lowercase().ends_with(".cpl") {
                // .cpl 文件是控制面板扩展，需要通过 control.exe 或 rundll32 打开
                // 使用 control.exe 是最可靠的方式
                Command::new("control.exe")
                    .arg(&tool_path)
                    .spawn()
            } else {
                Command::new(&tool_path).spawn()
            };

            match result {
                Ok(_) => {
                    self.tool_message = format!("已启动: {}", tool_name);
                }
                Err(e) => {
                    self.tool_message = format!("启动失败: {} - {}", tool_name, e);
                }
            }
        } else {
            self.tool_message = format!("工具不存在: {:?}", tool_path);
        }
    }

    fn launch_ghost_tool(&mut self) {
        let bin_dir = crate::utils::path::get_bin_dir();
        let ghost_path = bin_dir.join("ghost").join("Ghost64.exe");

        if ghost_path.exists() {
            match Command::new(&ghost_path).spawn() {
                Ok(_) => {
                    self.tool_message = "已启动: Ghost64.exe".to_string();
                }
                Err(e) => {
                    self.tool_message = format!("启动失败: Ghost64.exe - {}", e);
                }
            }
        } else {
            self.tool_message = format!("工具不存在: {:?}", ghost_path);
        }
    }

    fn repair_boot_action(&mut self, is_pe: bool) {
        let target_partition = if is_pe {
            // PE环境下使用用户选择的分区
            match &self.tool_target_partition {
                Some(p) => p.clone(),
                None => {
                    self.tool_message = "请先选择目标系统分区".to_string();
                    return;
                }
            }
        } else {
            // 正常环境下使用当前系统盘
            std::env::var("SystemDrive").unwrap_or_else(|_| "C:".to_string())
        };

        let boot_manager = crate::core::bcdedit::BootManager::new();

        match boot_manager.repair_boot(&target_partition) {
            Ok(_) => {
                self.tool_message = format!("引导修复成功: {}", target_partition);
            }
            Err(e) => {
                self.tool_message = format!("引导修复失败: {}", e);
            }
        }
    }

    fn export_drivers_action(&mut self, is_pe: bool) {
        let dism = crate::core::dism::Dism::new();
        let export_dir = crate::utils::path::get_exe_dir()
            .join("drivers_backup")
            .to_string_lossy()
            .to_string();

        self.tool_message = "正在导出驱动...".to_string();

        if is_pe {
            // PE环境下使用离线方式导出
            let source_partition = match &self.tool_target_partition {
                Some(p) => p.clone(),
                None => {
                    self.tool_message = "请先选择源系统分区".to_string();
                    return;
                }
            };

            match dism.export_drivers_from_system(&source_partition, &export_dir) {
                Ok(_) => {
                    self.tool_message = format!("驱动导出成功: {} -> {}", source_partition, export_dir);
                }
                Err(e) => {
                    self.tool_message = format!("驱动导出失败: {}", e);
                }
            }
        } else {
            // 正常环境下使用在线方式导出
            match dism.export_drivers(&export_dir) {
                Ok(_) => {
                    self.tool_message = format!("驱动导出成功: {}", export_dir);
                }
                Err(e) => {
                    self.tool_message = format!("驱动导出失败: {}", e);
                }
            }
        }
    }
}
