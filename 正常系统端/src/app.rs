use eframe::egui;
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};

use crate::core::disk::Partition;
use crate::core::dism::{DismProgress, ImageInfo};
use crate::core::hardware_info::HardwareInfo;
use crate::core::system_info::SystemInfo;
use crate::download::aria2::DownloadProgress;
use crate::download::config::ConfigManager;
use crate::download::manager::DownloadManager;
use crate::ui::advanced_options::AdvancedOptions;

/// 应用面板
#[derive(Debug, Clone, PartialEq)]
pub enum Panel {
    SystemInstall,
    SystemBackup,
    OnlineDownload,
    Tools,
    HardwareInfo,
    DownloadProgress,
    InstallProgress,
    BackupProgress,
    About,
}

/// 安装进度
#[derive(Debug, Clone, Default)]
pub struct InstallProgress {
    pub current_step: String,
    pub step_progress: u8,
    pub total_progress: u8,
}

/// 引导模式选择
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum BootModeSelection {
    #[default]
    Auto,
    UEFI,
    Legacy,
}

impl std::fmt::Display for BootModeSelection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BootModeSelection::Auto => write!(f, "自动"),
            BootModeSelection::UEFI => write!(f, "UEFI"),
            BootModeSelection::Legacy => write!(f, "Legacy"),
        }
    }
}

/// 安装模式
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum InstallMode {
    #[default]
    Direct,       // 直接安装（目标分区非当前系统分区，或在PE中）
    ViaPE,        // 通过PE安装（目标分区是当前系统分区）
}

/// 备份模式
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum BackupMode {
    #[default]
    Direct,       // 直接备份
    ViaPE,        // 通过PE备份
}

/// 安装选项
#[derive(Clone, Default)]
pub struct InstallOptions {
    pub format_partition: bool,
    pub repair_boot: bool,
    pub unattended_install: bool,
    pub export_drivers: bool,
    pub auto_reboot: bool,
    pub boot_mode: BootModeSelection,
    pub advanced_options: AdvancedOptions,
}

/// 主应用结构
pub struct App {
    // 当前选中的面板
    pub current_panel: Panel,

    // 系统信息
    pub system_info: Option<SystemInfo>,
    
    // 硬件信息
    pub hardware_info: Option<HardwareInfo>,
    pub hardware_info_loading: bool,

    // 磁盘分区列表
    pub partitions: Vec<Partition>,
    pub selected_partition: Option<usize>,

    // 在线资源
    pub config: Option<ConfigManager>,
    pub selected_online_system: Option<usize>,
    
    // 远程配置
    pub remote_config: Option<crate::download::server_config::RemoteConfig>,
    pub remote_config_loading: bool,
    
    // PE选择（用于安装/备份界面）
    pub selected_pe_for_install: Option<usize>,
    pub selected_pe_for_backup: Option<usize>,

    // 本地镜像
    pub local_image_path: String,
    pub image_volumes: Vec<ImageInfo>,
    pub selected_volume: Option<usize>,

    // 安装选项
    pub format_partition: bool,
    pub repair_boot: bool,
    pub unattended_install: bool,
    pub export_drivers: bool,
    pub auto_reboot: bool,
    pub selected_boot_mode: BootModeSelection,

    // 高级选项
    pub advanced_options: AdvancedOptions,
    pub show_advanced_options: bool,

    // 安装相关
    pub install_options: InstallOptions,
    pub install_target_partition: String,
    pub install_image_path: String,
    pub install_volume_index: u32,
    pub install_is_system_partition: bool,
    pub install_step: usize,
    pub install_mode: InstallMode,

    // 下载管理
    pub current_download: Option<String>,
    pub current_download_filename: Option<String>,
    pub download_progress: Option<DownloadProgress>,
    pub pending_download_url: Option<String>,
    pub pending_download_filename: Option<String>,
    pub download_save_path: String,

    // 安装进度
    pub install_progress: InstallProgress,
    pub is_installing: bool,

    // 备份相关
    pub backup_source_partition: Option<usize>,
    pub backup_save_path: String,
    pub backup_name: String,
    pub backup_description: String,
    pub backup_incremental: bool,
    pub is_backing_up: bool,
    pub backup_progress: u8,
    pub backup_mode: BackupMode,

    // 工具箱
    pub tool_message: String,
    pub tool_target_partition: Option<String>,

    // tokio 运行时
    pub runtime: tokio::runtime::Runtime,

    // 下载管理器
    pub download_manager: Arc<Mutex<Option<DownloadManager>>>,
    pub download_gid: Option<String>,
    pub download_progress_rx: Option<Receiver<DownloadProgress>>,
    pub download_init_error: Option<String>,

    // 备份进度通道
    pub backup_progress_rx: Option<Receiver<DismProgress>>,
    pub backup_error: Option<String>,

    // 安装进度通道
    pub install_progress_rx: Option<Receiver<DismProgress>>,
    pub install_error: Option<String>,

    // ISO 挂载状态
    pub iso_mounting: bool,
    pub iso_mount_error: Option<String>,
    
    // 镜像信息加载状态
    pub image_info_loading: bool,
    
    // PE 下载状态
    pub pe_downloading: bool,
    pub pe_download_error: Option<String>,
    
    // PE下载完成后继续的操作
    pub pe_download_then_action: Option<PeDownloadThenAction>,
    
    // 远程配置加载通道
    pub remote_config_rx: Option<Receiver<crate::download::server_config::RemoteConfig>>,
    
    // 下载完成后跳转到安装页面
    pub download_then_install: bool,
    pub download_then_install_path: Option<String>,
    
    // 软件下载后运行
    pub soft_download_then_run: bool,
    pub soft_download_then_run_path: Option<String>,
    
    // 在线下载页面选项卡
    pub online_download_tab: OnlineDownloadTab,
    
    // 软件下载相关
    pub soft_download_save_path: String,
    pub soft_download_run_after: bool,
    pub show_soft_download_modal: bool,
    pub pending_soft_download: Option<PendingSoftDownload>,
    
    // 软件图标缓存
    pub soft_icon_cache: std::collections::HashMap<String, SoftIconState>,
    pub soft_icon_loading: std::collections::HashSet<String>,
    
    // 错误对话框
    pub show_error_dialog: bool,
    pub error_dialog_message: String,
    
    // 网络信息对话框
    pub show_network_info_dialog: bool,
    pub network_info_cache: Option<Vec<crate::core::hardware_info::NetworkAdapterInfo>>,
}

/// 在线下载页面选项卡
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum OnlineDownloadTab {
    #[default]
    SystemImage,
    Software,
}

/// 待下载的软件信息
#[derive(Debug, Clone)]
pub struct PendingSoftDownload {
    pub name: String,
    pub download_url: String,
    pub filename: String,
}

/// 软件图标状态
#[derive(Clone)]
pub enum SoftIconState {
    Loading,
    Loaded(egui::TextureHandle),
    Failed,
}

/// PE下载完成后要执行的操作
#[derive(Debug, Clone)]
pub enum PeDownloadThenAction {
    Install,  // 继续安装
    Backup,   // 继续备份
}

impl Default for App {
    fn default() -> Self {
        let runtime = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");

        Self {
            current_panel: Panel::SystemInstall,
            system_info: None,
            hardware_info: None,
            hardware_info_loading: false,
            partitions: Vec::new(),
            selected_partition: None,
            config: None,
            selected_online_system: None,
            remote_config: None,
            remote_config_loading: false,
            selected_pe_for_install: None,
            selected_pe_for_backup: None,
            local_image_path: String::new(),
            image_volumes: Vec::new(),
            selected_volume: None,
            format_partition: true,
            repair_boot: true,
            unattended_install: true,
            export_drivers: true,
            auto_reboot: false,
            selected_boot_mode: BootModeSelection::Auto,
            advanced_options: AdvancedOptions::default(),
            show_advanced_options: false,
            install_options: InstallOptions::default(),
            install_target_partition: String::new(),
            install_image_path: String::new(),
            install_volume_index: 1,
            install_is_system_partition: false,
            install_step: 0,
            install_mode: InstallMode::Direct,
            current_download: None,
            current_download_filename: None,
            download_progress: None,
            pending_download_url: None,
            pending_download_filename: None,
            download_save_path: String::new(),
            install_progress: InstallProgress::default(),
            is_installing: false,
            backup_source_partition: None,
            backup_save_path: String::new(),
            backup_name: String::new(),
            backup_description: String::new(),
            backup_incremental: false,
            is_backing_up: false,
            backup_progress: 0,
            backup_mode: BackupMode::Direct,
            tool_message: String::new(),
            tool_target_partition: None,
            runtime,
            download_manager: Arc::new(Mutex::new(None)),
            download_gid: None,
            download_progress_rx: None,
            download_init_error: None,
            backup_progress_rx: None,
            backup_error: None,
            install_progress_rx: None,
            install_error: None,
            iso_mounting: false,
            iso_mount_error: None,
            image_info_loading: false,
            pe_downloading: false,
            pe_download_error: None,
            pe_download_then_action: None,
            remote_config_rx: None,
            download_then_install: false,
            download_then_install_path: None,
            soft_download_then_run: false,
            soft_download_then_run_path: None,
            online_download_tab: OnlineDownloadTab::default(),
            soft_download_save_path: String::new(),
            soft_download_run_after: true,
            show_soft_download_modal: false,
            pending_soft_download: None,
            soft_icon_cache: HashMap::new(),
            soft_icon_loading: HashSet::new(),
            show_error_dialog: false,
            error_dialog_message: String::new(),
            show_network_info_dialog: false,
            network_info_cache: None,
        }
    }
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // 设置中文字体
        Self::setup_fonts(&cc.egui_ctx);

        // 设置视觉样式
        Self::setup_style(&cc.egui_ctx);

        let mut app = Self::default();
        app.load_initial_data();
        app
    }

    /// 使用预加载的配置创建应用
    pub fn new_with_preloaded(cc: &eframe::CreationContext<'_>, preloaded: &crate::PreloadedConfig) -> Self {
        // 设置中文字体
        Self::setup_fonts(&cc.egui_ctx);

        // 设置视觉样式
        Self::setup_style(&cc.egui_ctx);

        let mut app = Self::default();
        app.load_initial_data_with_preloaded(preloaded);
        app
    }

    fn setup_fonts(ctx: &egui::Context) {
        let mut fonts = egui::FontDefinitions::default();

        // 动态获取 Windows 目录
        let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".to_string());
        let font_path = std::path::Path::new(&windir).join("Fonts").join("msyh.ttc");

        if let Ok(font_data) = std::fs::read(font_path) {
            fonts.font_data.insert(
                "msyh".to_owned(),
                std::sync::Arc::new(egui::FontData::from_owned(font_data)),
            );

            fonts
                .families
                .get_mut(&egui::FontFamily::Proportional)
                .unwrap()
                .insert(0, "msyh".to_owned());

            fonts
                .families
                .get_mut(&egui::FontFamily::Monospace)
                .unwrap()
                .insert(0, "msyh".to_owned());
        }

        ctx.set_fonts(fonts);
    }

    fn setup_style(ctx: &egui::Context) {
        let mut options = ctx.options(|o| o.clone());
        
        // 修改深色样式
        let mut dark_style = (*options.dark_style).clone();
        dark_style.text_styles = [
            (egui::TextStyle::Small, egui::FontId::proportional(12.0)),
            (egui::TextStyle::Body, egui::FontId::proportional(14.0)),
            (egui::TextStyle::Button, egui::FontId::proportional(14.0)),
            (egui::TextStyle::Heading, egui::FontId::proportional(20.0)),
            (egui::TextStyle::Monospace, egui::FontId::monospace(14.0)),
        ]
        .into();
        dark_style.spacing.item_spacing = egui::vec2(10.0, 8.0);
        dark_style.spacing.button_padding = egui::vec2(10.0, 5.0);
        
        // 修改浅色样式
        let mut light_style = (*options.light_style).clone();
        light_style.text_styles = [
            (egui::TextStyle::Small, egui::FontId::proportional(12.0)),
            (egui::TextStyle::Body, egui::FontId::proportional(14.0)),
            (egui::TextStyle::Button, egui::FontId::proportional(14.0)),
            (egui::TextStyle::Heading, egui::FontId::proportional(20.0)),
            (egui::TextStyle::Monospace, egui::FontId::monospace(14.0)),
        ]
        .into();
        light_style.spacing.item_spacing = egui::vec2(10.0, 8.0);
        light_style.spacing.button_padding = egui::vec2(10.0, 5.0);
        
        light_style.visuals.widgets.inactive.expansion = 0.0;
        light_style.visuals.widgets.hovered.expansion = 0.0;
        light_style.visuals.widgets.active.expansion = 0.0;
        light_style.visuals.widgets.open.expansion = 0.0;
        light_style.visuals.widgets.noninteractive.expansion = 0.0;
        
        options.dark_style = std::sync::Arc::new(dark_style);
        options.light_style = std::sync::Arc::new(light_style);
        ctx.options_mut(|o| *o = options);
    }

    fn load_initial_data(&mut self) {
        // 加载系统信息
        self.system_info = SystemInfo::collect().ok();

        // 加载硬件信息
        self.hardware_info = crate::core::hardware_info::HardwareInfo::collect().ok();

        // 加载分区列表
        self.partitions = crate::core::disk::DiskManager::get_partitions().unwrap_or_default();

        // 判断是否为PE环境
        let is_pe = self.system_info.as_ref().map(|s| s.is_pe_environment).unwrap_or(false);
        
        // 选择默认分区
        // 非PE环境：默认选择当前系统分区
        // PE环境：如果只有一个装有系统的分区则默认选择它，否则不默认选择
        if is_pe {
            // PE环境下，统计有系统的分区
            let windows_partitions: Vec<usize> = self.partitions
                .iter()
                .enumerate()
                .filter(|(_, p)| p.has_windows)
                .map(|(i, _)| i)
                .collect();
            
            if windows_partitions.len() == 1 {
                // 只有一个系统分区，默认选择它
                self.selected_partition = Some(windows_partitions[0]);
                self.backup_source_partition = Some(windows_partitions[0]);
            } else {
                // 有多个或没有系统分区，不默认选择
                self.selected_partition = None;
                self.backup_source_partition = None;
            }
        } else {
            // 非PE环境，选择当前系统分区
            let system_partition_idx = self.partitions.iter().position(|p| p.is_system_partition);
            self.selected_partition = system_partition_idx;
            self.backup_source_partition = system_partition_idx;
        }

        // 异步加载远程配置（不阻塞UI）
        log::info!("开始异步加载远程配置...");
        self.start_remote_config_loading();

        // 设置默认下载路径
        let exe_dir = crate::utils::path::get_exe_dir();
        self.download_save_path = exe_dir.join("downloads").to_string_lossy().to_string();

        // 设置默认备份名称
        self.backup_name = format!("系统备份_{}", chrono::Local::now().format("%Y%m%d_%H%M%S"));
        self.backup_description = "使用 LetRecovery 创建的系统备份".to_string();
    }

    /// 使用预加载的配置初始化数据
    fn load_initial_data_with_preloaded(&mut self, preloaded: &crate::PreloadedConfig) {
        // 使用预加载的系统信息
        self.system_info = preloaded.system_info.clone();

        // 使用预加载的硬件信息
        self.hardware_info = preloaded.hardware_info.clone();

        // 使用预加载的分区列表
        self.partitions = preloaded.partitions.clone();

        // 判断是否为PE环境
        let is_pe = self.system_info.as_ref().map(|s| s.is_pe_environment).unwrap_or(false);
        
        // 选择默认分区
        if is_pe {
            let windows_partitions: Vec<usize> = self.partitions
                .iter()
                .enumerate()
                .filter(|(_, p)| p.has_windows)
                .map(|(i, _)| i)
                .collect();
            
            if windows_partitions.len() == 1 {
                self.selected_partition = Some(windows_partitions[0]);
                self.backup_source_partition = Some(windows_partitions[0]);
            } else {
                self.selected_partition = None;
                self.backup_source_partition = None;
            }
        } else {
            let system_partition_idx = self.partitions.iter().position(|p| p.is_system_partition);
            self.selected_partition = system_partition_idx;
            self.backup_source_partition = system_partition_idx;
        }

        // 使用预加载的远程配置
        if let Some(ref remote_config) = preloaded.remote_config {
            self.remote_config_loading = false;
            
            if remote_config.loaded {
                self.config = Some(ConfigManager::load_from_content_with_soft(
                    remote_config.dl_content.as_deref(),
                    remote_config.pe_content.as_deref(),
                    remote_config.soft_content.as_deref(),
                ));
                log::info!("使用预加载的远程配置");
                
                // 自动选择第一个PE
                if let Some(ref config) = self.config {
                    if !config.pe_list.is_empty() {
                        if self.selected_pe_for_install.is_none() {
                            self.selected_pe_for_install = Some(0);
                        }
                        if self.selected_pe_for_backup.is_none() {
                            self.selected_pe_for_backup = Some(0);
                        }
                    }
                }
            } else {
                log::warn!("预加载的远程配置加载失败: {:?}", remote_config.error);
            }
            
            self.remote_config = Some(remote_config.clone());
        } else {
            // 如果没有预加载配置，则异步加载
            log::info!("没有预加载配置，开始异步加载远程配置...");
            self.start_remote_config_loading();
        }

        // 设置默认下载路径
        let exe_dir = crate::utils::path::get_exe_dir();
        self.download_save_path = exe_dir.join("downloads").to_string_lossy().to_string();

        // 设置默认备份名称
        self.backup_name = format!("系统备份_{}", chrono::Local::now().format("%Y%m%d_%H%M%S"));
        self.backup_description = "使用 LetRecovery 创建的系统备份".to_string();
    }
    
    /// 开始异步加载远程配置
    pub fn start_remote_config_loading(&mut self) {
        use std::sync::mpsc;
        
        if self.remote_config_loading {
            return; // 已经在加载中
        }
        
        self.remote_config_loading = true;
        
        let (tx, rx) = mpsc::channel::<crate::download::server_config::RemoteConfig>();
        self.remote_config_rx = Some(rx);
        
        std::thread::spawn(move || {
            let config = crate::download::server_config::RemoteConfig::load_from_server();
            let _ = tx.send(config);
        });
    }
    
    /// 检查远程配置加载状态
    pub fn check_remote_config_loading(&mut self) {
        if !self.remote_config_loading {
            return;
        }
        
        if let Some(ref rx) = self.remote_config_rx {
            if let Ok(remote_config) = rx.try_recv() {
                self.remote_config_loading = false;
                self.remote_config_rx = None;
                
                if remote_config.loaded {
                    self.config = Some(ConfigManager::load_from_content_with_soft(
                        remote_config.dl_content.as_deref(),
                        remote_config.pe_content.as_deref(),
                        remote_config.soft_content.as_deref(),
                    ));
                    log::info!("远程配置加载成功");
                    
                    // 自动选择第一个PE
                    if let Some(ref config) = self.config {
                        if !config.pe_list.is_empty() {
                            if self.selected_pe_for_install.is_none() {
                                self.selected_pe_for_install = Some(0);
                            }
                            if self.selected_pe_for_backup.is_none() {
                                self.selected_pe_for_backup = Some(0);
                            }
                        }
                    }
                } else {
                    log::warn!("远程配置加载失败: {:?}", remote_config.error);
                    // 远程配置加载失败，相关功能将被禁用
                }
                
                self.remote_config = Some(remote_config);
            }
        }
    }

    /// 检查PE配置是否可用
    pub fn is_pe_config_available(&self) -> bool {
        self.config.as_ref().map(|c| !c.pe_list.is_empty()).unwrap_or(false)
    }

    /// 检查是否在PE环境中
    pub fn is_pe_environment(&self) -> bool {
        self.system_info.as_ref().map(|s| s.is_pe_environment).unwrap_or(false)
    }

    /// 显示错误对话框
    pub fn show_error(&mut self, message: &str) {
        self.error_dialog_message = message.to_string();
        self.show_error_dialog = true;
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 检查远程配置加载状态
        self.check_remote_config_loading();
        
        // 处理图标加载结果
        self.process_icon_load_results(ctx);
        
        // 错误对话框
        if self.show_error_dialog {
            egui::Window::new("错误")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(10.0);
                        ui.colored_label(egui::Color32::RED, "❌");
                        ui.add_space(10.0);
                        ui.label(&self.error_dialog_message);
                        ui.add_space(20.0);
                        if ui.button("确定").clicked() {
                            self.show_error_dialog = false;
                            self.error_dialog_message.clear();
                        }
                        ui.add_space(10.0);
                    });
                });
        }

        // 底部状态栏
        egui::TopBottomPanel::bottom("bottom_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if let Some(info) = &self.system_info {
                    ui.label(format!(
                        "启动模式: {} | TPM: {} {} | 安全启动: {} | {}",
                        info.boot_mode,
                        if info.tpm_enabled {
                            "已启用"
                        } else {
                            "已禁用"
                        },
                        if !info.tpm_version.is_empty() {
                            format!("v{}", info.tpm_version)
                        } else {
                            String::new()
                        },
                        if info.secure_boot {
                            "已开启"
                        } else {
                            "已关闭"
                        },
                        if info.is_pe_environment {
                            "PE环境"
                        } else {
                            "桌面环境"
                        },
                    ));
                }
            });
        });

        // 左侧导航栏
        egui::SidePanel::left("nav_panel")
            .min_width(150.0)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.heading("LetRecovery");
                });

                ui.add_space(20.0);

                // 检查是否有操作正在进行
                let is_busy = self.is_installing || self.is_backing_up || self.current_download.is_some();

                if is_busy {
                    ui.colored_label(
                        egui::Color32::from_rgb(255, 165, 0),
                        "⚠ 操作进行中...",
                    );
                    ui.add_space(5.0);
                }

                if ui
                    .add_enabled(
                        !is_busy || self.current_panel == Panel::SystemInstall,
                        egui::SelectableLabel::new(self.current_panel == Panel::SystemInstall, "系统安装"),
                    )
                    .clicked()
                {
                    self.current_panel = Panel::SystemInstall;
                }

                if ui
                    .add_enabled(
                        !is_busy || self.current_panel == Panel::SystemBackup,
                        egui::SelectableLabel::new(self.current_panel == Panel::SystemBackup, "系统备份"),
                    )
                    .clicked()
                {
                    self.current_panel = Panel::SystemBackup;
                }

                if ui
                    .add_enabled(
                        !is_busy || self.current_panel == Panel::OnlineDownload,
                        egui::SelectableLabel::new(self.current_panel == Panel::OnlineDownload, "在线下载"),
                    )
                    .clicked()
                {
                    self.current_panel = Panel::OnlineDownload;
                }

                if ui
                    .add_enabled(
                        !is_busy || self.current_panel == Panel::Tools,
                        egui::SelectableLabel::new(self.current_panel == Panel::Tools, "工具箱"),
                    )
                    .clicked()
                {
                    self.current_panel = Panel::Tools;
                }

                if ui
                    .add_enabled(
                        !is_busy || self.current_panel == Panel::HardwareInfo,
                        egui::SelectableLabel::new(self.current_panel == Panel::HardwareInfo, "硬件信息"),
                    )
                    .clicked()
                {
                    self.current_panel = Panel::HardwareInfo;
                }

                if ui
                    .add_enabled(
                        !is_busy || self.current_panel == Panel::About,
                        egui::SelectableLabel::new(self.current_panel == Panel::About, "关于"),
                    )
                    .clicked()
                {
                    self.current_panel = Panel::About;
                }
            });

        // 主面板
        egui::CentralPanel::default().show(ctx, |ui| match self.current_panel {
            Panel::SystemInstall => self.show_system_install(ui),
            Panel::SystemBackup => self.show_system_backup(ui),
            Panel::OnlineDownload => self.show_online_download(ui),
            Panel::Tools => self.show_tools(ui),
            Panel::HardwareInfo => self.show_hardware_info(ui),
            Panel::DownloadProgress => self.show_download_progress(ui),
            Panel::InstallProgress => self.show_install_progress(ui),
            Panel::BackupProgress => self.show_backup_progress(ui),
            Panel::About => self.show_about(ui),
        });

        // 高级选项窗口
        if self.show_advanced_options {
            egui::Window::new("高级选项")
                .open(&mut self.show_advanced_options)
                .min_width(500.0)
                .min_height(400.0)
                .show(ctx, |ui| {
                    self.advanced_options.show_ui(ui);
                });
        }

        // 如果有正在进行的任务，定期刷新
        if self.is_installing || self.is_backing_up || self.current_download.is_some() || self.iso_mounting || self.pe_downloading || self.remote_config_loading {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
}
