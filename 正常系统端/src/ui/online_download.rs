use egui;

use crate::app::App;
use crate::download::config::{OnlinePE, OnlineSystem};

impl App {
    pub fn show_online_download(&mut self, ui: &mut egui::Ui) {
        ui.heading("在线下载");
        ui.separator();

        if self.config.is_none() || self.config.as_ref().map(|c| c.is_empty()).unwrap_or(true) {
            ui.colored_label(egui::Color32::from_rgb(255, 165, 0), "未找到在线资源配置");
            ui.label("请确保程序目录下存在 dl.txt 和 pe.txt 配置文件");

            if ui.button("刷新配置").clicked() {
                self.load_online_config();
            }
            return;
        }

        // 克隆配置以避免借用冲突
        let systems: Vec<OnlineSystem> = self
            .config
            .as_ref()
            .map(|c| c.systems.clone())
            .unwrap_or_default();
        let pe_list: Vec<OnlinePE> = self
            .config
            .as_ref()
            .map(|c| c.pe_list.clone())
            .unwrap_or_default();

        // 系统镜像列表
        ui.heading("系统镜像");

        let mut system_to_download: Option<usize> = None;
        let mut system_selected: Option<usize> = None;

        egui::ScrollArea::vertical()
            .max_height(200.0)
            .id_salt("system_list")
            .show(ui, |ui| {
                egui::Grid::new("system_grid")
                    .striped(true)
                    .min_col_width(200.0)
                    .show(ui, |ui| {
                        ui.label("系统名称");
                        ui.label("类型");
                        ui.label("操作");
                        ui.end_row();

                        for (i, system) in systems.iter().enumerate() {
                            if ui
                                .selectable_label(
                                    self.selected_online_system == Some(i),
                                    &system.display_name,
                                )
                                .clicked()
                            {
                                system_selected = Some(i);
                            }

                            ui.label(if system.is_win11 { "Win11" } else { "Win10" });

                            if ui.button("下载").clicked() {
                                system_to_download = Some(i);
                            }
                            ui.end_row();
                        }
                    });
            });

        // 处理选择
        if let Some(i) = system_selected {
            self.selected_online_system = Some(i);
        }

        // 处理下载
        if let Some(i) = system_to_download {
            if let Some(system) = systems.get(i) {
                self.pending_download_url = Some(system.download_url.clone());
                // 不指定文件名，让aria2自动从服务器Content-Disposition或URL获取
                self.pending_download_filename = None;
                self.current_panel = crate::app::Panel::DownloadProgress;
            }
        }

        ui.add_space(15.0);
        ui.separator();

        // PE 镜像列表
        ui.heading("PE 环境");

        let mut pe_to_download: Option<usize> = None;

        egui::ScrollArea::vertical()
            .max_height(150.0)
            .id_salt("pe_list")
            .show(ui, |ui| {
                egui::Grid::new("pe_grid")
                    .striped(true)
                    .min_col_width(200.0)
                    .show(ui, |ui| {
                        ui.label("PE名称");
                        ui.label("文件名");
                        ui.label("状态");
                        ui.label("操作");
                        ui.end_row();

                        for (i, pe) in pe_list.iter().enumerate() {
                            ui.label(&pe.display_name);
                            ui.label(&pe.filename);
                            
                            // 检查PE文件是否已存在
                            let (exists, _) = crate::core::pe::PeManager::check_pe_exists(&pe.filename);
                            if exists {
                                ui.colored_label(egui::Color32::GREEN, "✓ 已下载");
                            } else {
                                ui.colored_label(egui::Color32::GRAY, "未下载");
                            }

                            if ui.button("下载").clicked() {
                                pe_to_download = Some(i);
                            }
                            ui.end_row();
                        }
                    });
            });

        // 处理 PE 下载
        if let Some(i) = pe_to_download {
            if let Some(pe) = pe_list.get(i) {
                self.pending_download_url = Some(pe.download_url.clone());
                self.pending_download_filename = Some(pe.filename.clone());
                // PE下载到程序目录下的PE文件夹
                let pe_dir = crate::utils::path::get_exe_dir()
                    .join("PE")
                    .to_string_lossy()
                    .to_string();
                self.download_save_path = pe_dir;
                self.current_panel = crate::app::Panel::DownloadProgress;
            }
        }

        ui.add_space(15.0);
        ui.separator();

        // 下载保存位置
        ui.horizontal(|ui| {
            ui.label("保存位置:");
            ui.add(
                egui::TextEdit::singleline(&mut self.download_save_path).desired_width(400.0),
            );
            if ui.button("浏览...").clicked() {
                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                    self.download_save_path = path.to_string_lossy().to_string();
                }
            }
        });

        // 刷新按钮
        ui.add_space(10.0);
        if ui.button("刷新在线资源").clicked() {
            self.load_online_config();
        }
    }

    pub fn load_online_config(&mut self) {
        let exe_dir = crate::utils::path::get_exe_dir();
        if let Ok(config) = crate::download::config::ConfigManager::load_from_local(&exe_dir) {
            self.config = Some(config);
        }
    }
}
