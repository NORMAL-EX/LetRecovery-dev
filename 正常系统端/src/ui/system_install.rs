use egui;
use std::sync::mpsc;

use crate::app::{App, BootModeSelection, InstallMode};
use crate::core::disk::{Partition, PartitionStyle};
use crate::core::dism::ImageInfo;

/// ISO 挂载结果
pub enum IsoMountResult {
    Success(String),
    Error(String),
}

/// 镜像信息加载结果
pub enum ImageInfoResult {
    Success(Vec<ImageInfo>),
    Error(String),
}

impl App {
    pub fn show_system_install(&mut self, ui: &mut egui::Ui) {
        ui.heading("系统安装");
        ui.separator();

        let is_pe = self.is_pe_environment();
        
        // 判断是否需要通过PE安装
        let needs_pe = self.check_if_needs_pe_for_install();
        
        // 检查PE配置是否可用（仅在需要PE时检查）
        let pe_available = self.is_pe_config_available();
        
        // 在非PE环境且目标是系统分区时，需要显示PE选择
        let show_pe_selector = !is_pe && needs_pe;
        
        // 安装按钮是否可用
        let install_blocked = show_pe_selector && !pe_available;

        // 检查ISO挂载状态
        self.check_iso_mount_status();

        // 镜像文件选择
        ui.horizontal(|ui| {
            ui.label("系统镜像:");
            
            let text_edit = egui::TextEdit::singleline(&mut self.local_image_path)
                .desired_width(400.0);
            ui.add_enabled(!self.iso_mounting, text_edit);
            
            if ui.add_enabled(!self.iso_mounting, egui::Button::new("浏览...")).clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("系统镜像", &["wim", "esd", "iso", "gho"])
                    .pick_file()
                {
                    self.local_image_path = path.to_string_lossy().to_string();
                    self.iso_mount_error = None;
                    self.load_image_volumes();
                }
            }
        });

        // 显示ISO挂载状态
        if self.iso_mounting {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("正在挂载 ISO 镜像，请稍候...");
            });
        }

        // 显示镜像信息加载状态
        if self.image_info_loading {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("正在加载镜像信息，请稍候...");
            });
        }

        // 显示ISO挂载错误
        if let Some(ref error) = self.iso_mount_error {
            ui.colored_label(egui::Color32::RED, format!("ISO 挂载失败: {}", error));
        }

        // 镜像分卷选择
        if !self.image_volumes.is_empty() {
            ui.horizontal(|ui| {
                ui.label("系统版本:");
                egui::ComboBox::from_id_salt("volume_select")
                    .selected_text(
                        self.selected_volume
                            .and_then(|i| self.image_volumes.get(i))
                            .map(|v| v.name.as_str())
                            .unwrap_or("请选择版本"),
                    )
                    .show_ui(ui, |ui| {
                        for (i, vol) in self.image_volumes.iter().enumerate() {
                            ui.selectable_value(
                                &mut self.selected_volume,
                                Some(i),
                                format!("{} - {}", vol.index, vol.name),
                            );
                        }
                    });
            });
        }

        ui.add_space(10.0);
        ui.separator();

        // 分区选择表格
        ui.label("选择安装分区:");

        let partitions_clone: Vec<Partition> = self.partitions.clone();
        let mut partition_clicked: Option<usize> = None;

        egui::ScrollArea::vertical()
            .max_height(200.0)
            .show(ui, |ui| {
                egui::Grid::new("partition_grid")
                    .striped(true)
                    .min_col_width(60.0)
                    .show(ui, |ui| {
                        ui.label("分区卷");
                        ui.label("总空间");
                        ui.label("可用空间");
                        ui.label("卷标");
                        ui.label("分区表");
                        ui.label("状态");
                        ui.end_row();

                        for (i, partition) in partitions_clone.iter().enumerate() {
                            let label = if is_pe {
                                if partition.has_windows {
                                    format!("{} (有系统)", partition.letter)
                                } else {
                                    partition.letter.clone()
                                }
                            } else {
                                if partition.is_system_partition {
                                    format!("{} (当前系统)", partition.letter)
                                } else if partition.has_windows {
                                    format!("{} (有系统)", partition.letter)
                                } else {
                                    partition.letter.clone()
                                }
                            };

                            if ui
                                .selectable_label(self.selected_partition == Some(i), &label)
                                .clicked()
                            {
                                partition_clicked = Some(i);
                            }

                            ui.label(Self::format_size(partition.total_size_mb));
                            ui.label(Self::format_size(partition.free_size_mb));
                            ui.label(&partition.label);
                            ui.label(format!("{}", partition.partition_style));
                            
                            let status = if partition.has_windows {
                                "已有系统"
                            } else {
                                "空闲"
                            };
                            ui.label(status);
                            
                            ui.end_row();
                        }
                    });
            });

        // 处理分区选择
        if let Some(i) = partition_clicked {
            self.selected_partition = Some(i);
            self.update_install_options_for_partition();
        }

        ui.add_space(10.0);
        ui.separator();

        // 安装选项
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.format_partition, "格式化分区");
            ui.checkbox(&mut self.repair_boot, "添加引导");
            ui.checkbox(&mut self.unattended_install, "无人值守");
            ui.checkbox(&mut self.export_drivers, "保留驱动");
            ui.checkbox(&mut self.auto_reboot, "立即重启");
        });

        // 引导模式选择
        ui.horizontal(|ui| {
            ui.label("引导模式:");
            egui::ComboBox::from_id_salt("boot_mode_select")
                .selected_text(format!("{}", self.selected_boot_mode))
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.selected_boot_mode,
                        BootModeSelection::Auto,
                        "自动 (根据分区表)",
                    );
                    ui.selectable_value(
                        &mut self.selected_boot_mode,
                        BootModeSelection::UEFI,
                        "UEFI",
                    );
                    ui.selectable_value(
                        &mut self.selected_boot_mode,
                        BootModeSelection::Legacy,
                        "Legacy (BIOS)",
                    );
                });

            if let Some(idx) = self.selected_partition {
                if let Some(partition) = self.partitions.get(idx) {
                    let actual_mode = Self::get_actual_boot_mode(self.selected_boot_mode, partition.partition_style);
                    ui.label(format!("( 将使用: {} )", actual_mode));
                }
            }
        });

        // PE选择（仅在需要通过PE安装时显示）
        if show_pe_selector {
            ui.add_space(10.0);
            ui.separator();
            
            ui.horizontal(|ui| {
                ui.label("🔧 PE环境:");
                
                if pe_available {
                    if let Some(ref config) = self.config {
                        egui::ComboBox::from_id_salt("pe_select_install")
                            .selected_text(
                                self.selected_pe_for_install
                                    .and_then(|i| config.pe_list.get(i))
                                    .map(|p| p.display_name.as_str())
                                    .unwrap_or("请选择PE"),
                            )
                            .show_ui(ui, |ui| {
                                for (i, pe) in config.pe_list.iter().enumerate() {
                                    ui.selectable_value(
                                        &mut self.selected_pe_for_install,
                                        Some(i),
                                        &pe.display_name,
                                    );
                                }
                            });
                        
                        // 显示PE就绪状态
                        if let Some(idx) = self.selected_pe_for_install {
                            if let Some(pe) = config.pe_list.get(idx) {
                                let (exists, _) = crate::core::pe::PeManager::check_pe_exists(&pe.filename);
                                if exists {
                                    ui.colored_label(egui::Color32::GREEN, "✓ 已就绪");
                                } else {
                                    ui.colored_label(egui::Color32::from_rgb(255, 165, 0), "需下载");
                                }
                            }
                        }
                    }
                } else {
                    ui.colored_label(egui::Color32::RED, "未找到PE配置");
                }
            });
            
            ui.colored_label(
                egui::Color32::from_rgb(255, 165, 0),
                "⚠ 安装到当前系统分区需要先重启到PE环境",
            );
        }

        // PE配置缺失警告
        if install_blocked {
            ui.add_space(5.0);
            ui.colored_label(
                egui::Color32::RED,
                "❌ 缺少PE配置文件(pe.txt)，无法安装到当前系统分区。请重新下载该软件的完整版本后重试。",
            );
        }

        ui.horizontal(|ui| {
            if ui.button("高级选项...").clicked() {
                self.show_advanced_options = true;
            }
            if ui.button("刷新分区").clicked() {
                self.refresh_partitions();
            }
        });

        ui.add_space(20.0);

        // 开始安装按钮
        let can_install = self.selected_partition.is_some()
            && !self.local_image_path.is_empty()
            && (self.local_image_path.ends_with(".gho") || self.selected_volume.is_some())
            && !install_blocked
            && (!show_pe_selector || self.selected_pe_for_install.is_some());

        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    can_install && !self.is_installing,
                    egui::Button::new("开始安装").min_size(egui::vec2(120.0, 35.0)),
                )
                .clicked()
            {
                self.start_installation();
            }

            // 显示安装模式提示
            if can_install {
                if needs_pe && !is_pe {
                    ui.label("(将通过PE环境安装)");
                } else {
                    ui.label("(直接安装)");
                }
            }
        });

        // 警告：安装到有系统的分区
        if let Some(idx) = self.selected_partition {
            if let Some(partition) = self.partitions.get(idx) {
                if partition.has_windows && !self.format_partition {
                    ui.add_space(5.0);
                    ui.colored_label(
                        egui::Color32::from_rgb(255, 165, 0),
                        "⚠ 目标分区已有系统，建议勾选\"格式化分区\"",
                    );
                }
            }
        }
    }

    /// 检查是否需要通过PE安装
    fn check_if_needs_pe_for_install(&self) -> bool {
        // 如果已经在PE环境中，不需要再进PE
        if self.is_pe_environment() {
            return false;
        }
        
        // 检查目标分区是否是当前系统分区
        if let Some(idx) = self.selected_partition {
            if let Some(partition) = self.partitions.get(idx) {
                return partition.is_system_partition;
            }
        }
        
        false
    }

    /// 根据选择和分区表类型获取实际的引导模式
    fn get_actual_boot_mode(selection: BootModeSelection, partition_style: PartitionStyle) -> &'static str {
        match selection {
            BootModeSelection::UEFI => "UEFI",
            BootModeSelection::Legacy => "Legacy",
            BootModeSelection::Auto => {
                match partition_style {
                    PartitionStyle::GPT => "UEFI",
                    PartitionStyle::MBR => "Legacy",
                    PartitionStyle::Unknown => "UEFI",
                }
            }
        }
    }

    pub fn load_image_volumes(&mut self) {
        if self.local_image_path.to_lowercase().ends_with(".iso") {
            self.start_iso_mount();
            return;
        }

        // 其他格式直接后台加载
        self.start_image_info_loading(&self.local_image_path.clone());
    }

    fn start_image_info_loading(&mut self, image_path: &str) {
        let path_lower = image_path.to_lowercase();
        
        if path_lower.ends_with(".wim") || path_lower.ends_with(".esd") {
            println!("[IMAGE INFO] 开始后台加载镜像信息: {}", image_path);
            
            self.image_info_loading = true;
            self.image_volumes.clear();
            self.selected_volume = None;

            let (tx, rx) = mpsc::channel::<ImageInfoResult>();
            
            unsafe {
                IMAGE_INFO_RESULT_RX = Some(rx);
            }

            let path = image_path.to_string();

            std::thread::spawn(move || {
                println!("[IMAGE INFO THREAD] 线程启动，加载: {}", path);
                
                let dism = crate::core::dism::Dism::new();
                match dism.get_image_info(&path) {
                    Ok(volumes) => {
                        println!("[IMAGE INFO THREAD] 成功加载 {} 个卷", volumes.len());
                        let _ = tx.send(ImageInfoResult::Success(volumes));
                    }
                    Err(e) => {
                        println!("[IMAGE INFO THREAD] 加载失败: {}", e);
                        let _ = tx.send(ImageInfoResult::Error(e.to_string()));
                    }
                }
            });
        } else if path_lower.ends_with(".gho") || path_lower.ends_with(".ghs") {
            // GHO 文件不需要加载卷信息
            self.image_volumes.clear();
            self.selected_volume = Some(0);
        }
    }

    fn start_iso_mount(&mut self) {
        println!("[ISO MOUNT] 开始后台挂载 ISO: {}", self.local_image_path);
        
        self.iso_mounting = true;
        self.iso_mount_error = None;

        let (tx, rx) = mpsc::channel::<IsoMountResult>();
        
        unsafe {
            ISO_MOUNT_RESULT_RX = Some(rx);
        }

        let iso_path = self.local_image_path.clone();

        std::thread::spawn(move || {
            println!("[ISO MOUNT THREAD] 线程启动，挂载: {}", iso_path);
            
            match crate::core::iso::IsoMounter::mount_iso(&iso_path) {
                Ok(drive) => {
                    println!("[ISO MOUNT THREAD] 挂载成功，盘符: {}，查找安装镜像...", drive);
                    if let Some(image_path) = crate::core::iso::IsoMounter::find_install_image() {
                        println!("[ISO MOUNT THREAD] 找到镜像: {}", image_path);
                        let _ = tx.send(IsoMountResult::Success(image_path));
                    } else {
                        println!("[ISO MOUNT THREAD] 未找到安装镜像");
                        let _ = tx.send(IsoMountResult::Error("ISO 中未找到 install.wim/esd".to_string()));
                    }
                }
                Err(e) => {
                    println!("[ISO MOUNT THREAD] 挂载失败: {}", e);
                    let _ = tx.send(IsoMountResult::Error(e.to_string()));
                }
            }
        });
    }

    fn check_iso_mount_status(&mut self) {
        // 检查 ISO 挂载状态
        if self.iso_mounting {
            unsafe {
                if let Some(ref rx) = ISO_MOUNT_RESULT_RX {
                    if let Ok(result) = rx.try_recv() {
                        self.iso_mounting = false;
                        ISO_MOUNT_RESULT_RX = None;

                        match result {
                            IsoMountResult::Success(image_path) => {
                                println!("[ISO MOUNT] 挂载完成，镜像路径: {}", image_path);
                                self.local_image_path = image_path.clone();
                                self.iso_mount_error = None;
                                // 开始后台加载镜像信息
                                self.start_image_info_loading(&image_path);
                            }
                            IsoMountResult::Error(error) => {
                                println!("[ISO MOUNT] 挂载失败: {}", error);
                                self.iso_mount_error = Some(error);
                            }
                        }
                    }
                }
            }
        }

        // 检查镜像信息加载状态
        if self.image_info_loading {
            unsafe {
                if let Some(ref rx) = IMAGE_INFO_RESULT_RX {
                    if let Ok(result) = rx.try_recv() {
                        self.image_info_loading = false;
                        IMAGE_INFO_RESULT_RX = None;

                        match result {
                            ImageInfoResult::Success(volumes) => {
                                println!("[IMAGE INFO] 加载完成，找到 {} 个卷", volumes.len());
                                self.image_volumes = volumes;
                                self.selected_volume = if self.image_volumes.is_empty() {
                                    None
                                } else {
                                    Some(0)
                                };
                            }
                            ImageInfoResult::Error(error) => {
                                println!("[IMAGE INFO] 加载失败: {}", error);
                                self.image_volumes.clear();
                                self.selected_volume = None;
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn update_install_options_for_partition(&mut self) {
        if let Some(idx) = self.selected_partition {
            if let Some(partition) = self.partitions.get(idx) {
                if partition.has_windows || partition.is_system_partition {
                    self.format_partition = true;
                    self.repair_boot = true;
                }
            }
        }
    }

    pub fn format_size(size_mb: u64) -> String {
        if size_mb >= 1024 {
            format!("{:.1} GB", size_mb as f64 / 1024.0)
        } else {
            format!("{} MB", size_mb)
        }
    }

    pub fn refresh_partitions(&mut self) {
        if let Ok(partitions) = crate::core::disk::DiskManager::get_partitions() {
            self.partitions = partitions;
            self.selected_partition = self
                .partitions
                .iter()
                .position(|p| p.is_system_partition);
        }
    }

    pub fn start_installation(&mut self) {
        let partition = self
            .partitions
            .get(self.selected_partition.unwrap())
            .cloned();
        if partition.is_none() {
            return;
        }
        let partition = partition.unwrap();

        let image_path = self.local_image_path.clone();
        let volume_index = self
            .selected_volume
            .and_then(|i| self.image_volumes.get(i).map(|v| v.index))
            .unwrap_or(1);

        let is_system_partition = partition.is_system_partition;
        let is_pe = self.is_pe_environment();

        // 确定安装模式
        self.install_mode = if is_pe || !is_system_partition {
            InstallMode::Direct
        } else {
            InstallMode::ViaPE
        };

        // 如果需要通过PE安装，先检查PE是否存在
        if self.install_mode == InstallMode::ViaPE {
            let pe_info = self.selected_pe_for_install.and_then(|idx| {
                self.config.as_ref().and_then(|c| c.pe_list.get(idx).cloned())
            });
            
            if let Some(pe) = pe_info {
                let (pe_exists, _) = crate::core::pe::PeManager::check_pe_exists(&pe.filename);
                if !pe_exists {
                    // PE不存在，先下载PE
                    println!("[INSTALL] PE文件不存在，开始下载: {}", pe.filename);
                    self.pending_download_url = Some(pe.download_url.clone());
                    self.pending_download_filename = Some(pe.filename.clone());
                    let pe_dir = crate::utils::path::get_exe_dir()
                        .join("PE")
                        .to_string_lossy()
                        .to_string();
                    self.download_save_path = pe_dir;
                    self.pe_download_then_action = Some(crate::app::PeDownloadThenAction::Install);
                    self.current_panel = crate::app::Panel::DownloadProgress;
                    return;
                }
            }
        }

        self.install_options = crate::app::InstallOptions {
            format_partition: self.format_partition,
            repair_boot: self.repair_boot,
            unattended_install: self.unattended_install,
            export_drivers: self.export_drivers,
            auto_reboot: self.auto_reboot,
            boot_mode: self.selected_boot_mode,
            advanced_options: self.advanced_options.clone(),
        };

        self.is_installing = true;
        self.current_panel = crate::app::Panel::InstallProgress;
        self.install_progress = crate::app::InstallProgress::default();

        self.install_target_partition = partition.letter.clone();
        self.install_image_path = image_path;
        self.install_volume_index = volume_index;
        self.install_is_system_partition = is_system_partition;

        self.install_step = 0;
    }
}

static mut ISO_MOUNT_RESULT_RX: Option<mpsc::Receiver<IsoMountResult>> = None;
static mut IMAGE_INFO_RESULT_RX: Option<mpsc::Receiver<ImageInfoResult>> = None;
