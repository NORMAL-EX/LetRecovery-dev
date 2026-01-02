use egui;

use crate::app::App;

impl App {
    pub fn show_hardware_info(&mut self, ui: &mut egui::Ui) {
        ui.heading("硬件信息");
        ui.separator();

        // 系统信息
        if let Some(info) = &self.system_info {
            // PE 环境提示
            if info.is_pe_environment {
                ui.colored_label(
                    egui::Color32::from_rgb(100, 200, 255),
                    "🖥 当前运行在 PE 环境中",
                );
                ui.add_space(10.0);
            }

            egui::Grid::new("system_info_grid")
                .num_columns(2)
                .spacing([40.0, 8.0])
                .show(ui, |ui| {
                    ui.label("启动模式:");
                    ui.label(format!("{}", info.boot_mode));
                    ui.end_row();

                    ui.label("TPM状态:");
                    ui.label(if info.tpm_enabled {
                        format!("已启用 (版本 {})", info.tpm_version)
                    } else {
                        "未启用/未检测到".to_string()
                    });
                    ui.end_row();

                    ui.label("安全启动:");
                    ui.label(if info.secure_boot {
                        "已开启"
                    } else {
                        "已关闭/未检测到"
                    });
                    ui.end_row();

                    ui.label("系统架构:");
                    ui.label(if info.is_64bit { "64位" } else { "32位" });
                    ui.end_row();

                    ui.label("运行环境:");
                    ui.label(if info.is_pe_environment {
                        "PE环境"
                    } else {
                        "桌面环境"
                    });
                    ui.end_row();

                    ui.label("网络状态:");
                    ui.label(if info.is_online { "已联网" } else { "未联网" });
                    ui.end_row();
                });

            // PE 环境下的额外提示
            if info.is_pe_environment {
                ui.add_space(10.0);
                ui.separator();
                ui.label("PE 环境说明:");
                ui.label("• TPM 和安全启动状态可能无法准确检测");
                ui.label("• 部分系统工具可能不可用");
                ui.label("• 建议在\"工具箱\"中选择目标分区后操作");
            }
        } else {
            ui.label("正在获取系统信息...");
        }

        ui.add_space(20.0);
        ui.separator();

        // 磁盘分区信息
        ui.heading("磁盘分区");

        let is_pe = self.system_info.as_ref().map(|s| s.is_pe_environment).unwrap_or(false);

        egui::ScrollArea::vertical()
            .max_height(250.0)
            .show(ui, |ui| {
                egui::Grid::new("disk_info_grid")
                    .striped(true)
                    .min_col_width(70.0)
                    .show(ui, |ui| {
                        ui.label("分区");
                        ui.label("卷标");
                        ui.label("总容量");
                        ui.label("可用空间");
                        ui.label("已用空间");
                        ui.label("使用率");
                        ui.label("系统");
                        ui.end_row();

                        for partition in &self.partitions {
                            let used = partition.total_size_mb - partition.free_size_mb;
                            let usage = if partition.total_size_mb > 0 {
                                (used as f64 / partition.total_size_mb as f64) * 100.0
                            } else {
                                0.0
                            };

                            // 构建分区标签
                            let label = if is_pe {
                                if partition.letter.to_uppercase() == "X:" {
                                    format!("{} (PE)", partition.letter)
                                } else if partition.has_windows {
                                    format!("{} (Win)", partition.letter)
                                } else {
                                    partition.letter.clone()
                                }
                            } else {
                                if partition.is_system_partition {
                                    format!("{} (系统)", partition.letter)
                                } else {
                                    partition.letter.clone()
                                }
                            };

                            ui.label(label);
                            ui.label(&partition.label);
                            ui.label(Self::format_size(partition.total_size_mb));
                            ui.label(Self::format_size(partition.free_size_mb));
                            ui.label(Self::format_size(used));
                            ui.label(format!("{:.1}%", usage));
                            ui.label(if partition.has_windows { "有" } else { "-" });
                            ui.end_row();
                        }
                    });
            });

        ui.add_space(15.0);

        // 刷新按钮
        if ui.button("刷新信息").clicked() {
            self.refresh_system_info();
        }
    }

    fn refresh_system_info(&mut self) {
        // 刷新系统信息
        if let Ok(info) = crate::core::system_info::SystemInfo::collect() {
            self.system_info = Some(info);
        }

        // 刷新分区信息
        if let Ok(partitions) = crate::core::disk::DiskManager::get_partitions() {
            self.partitions = partitions;
        }
    }
}
