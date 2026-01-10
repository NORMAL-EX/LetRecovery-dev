use egui;
use std::sync::mpsc;

use crate::app::App;
use crate::download::aria2::{Aria2Manager, DownloadProgress, DownloadStatus};

/// 下载控制命令
#[derive(Debug, Clone)]
pub enum DownloadCommand {
    Pause,
    Resume,
    Cancel,
}

impl App {
    pub fn show_download_progress(&mut self, ui: &mut egui::Ui) {
        ui.heading("下载进度");
        ui.separator();

        // 从channel接收进度更新
        self.update_download_progress();

        // 如果有待下载的任务，开始下载
        if let Some(url) = self.pending_download_url.take() {
            let filename = self.pending_download_filename.take();
            let save_path = if self.download_save_path.is_empty() {
                crate::utils::path::get_exe_dir()
                    .join("downloads")
                    .to_string_lossy()
                    .to_string()
            } else {
                self.download_save_path.clone()
            };

            // 创建下载目录
            let _ = std::fs::create_dir_all(&save_path);

            // 初始化 aria2 并开始下载
            self.start_download_task(&url, &save_path, filename.as_deref());
        }

        // 显示初始化错误
        if let Some(ref error) = self.download_init_error {
            ui.add_space(15.0);
            ui.colored_label(egui::Color32::RED, format!("错误: {}", error));
            ui.add_space(10.0);
            if ui.button("返回").clicked() {
                self.download_init_error = None;
                // 先获取待执行操作
                let action = self.pe_download_then_action.take();
                // 根据操作类型返回对应页面
                match action {
                    Some(crate::app::PeDownloadThenAction::Install) => {
                        self.current_panel = crate::app::Panel::SystemInstall;
                    }
                    Some(crate::app::PeDownloadThenAction::Backup) => {
                        self.current_panel = crate::app::Panel::SystemBackup;
                    }
                    None => {
                        self.current_panel = crate::app::Panel::OnlineDownload;
                    }
                }
            }
            return;
        }

        // 克隆需要的数据以避免借用冲突
        let progress_clone = self.download_progress.clone();
        let filename_clone = self.current_download_filename.clone();

        // 显示当前下载状态
        if let Some(progress) = progress_clone {
            ui.add_space(15.0);

            // 文件名
            if let Some(filename) = &filename_clone {
                ui.label(format!("文件: {}", filename));
            }

            // 进度条
            ui.add(
                egui::ProgressBar::new(progress.percentage as f32 / 100.0)
                    .show_percentage()
                    .animate(progress.status == DownloadStatus::Active),
            );

            // 详细信息
            ui.horizontal(|ui| {
                ui.label(format!(
                    "已下载: {} / {}",
                    Self::format_bytes(progress.completed_length),
                    Self::format_bytes(progress.total_length)
                ));
                ui.separator();
                ui.label(format!(
                    "速度: {}/s",
                    Self::format_bytes(progress.download_speed)
                ));
            });

            // 状态
            let status_text = match &progress.status {
                DownloadStatus::Waiting => "等待中...",
                DownloadStatus::Active => "下载中...",
                DownloadStatus::Paused => "已暂停",
                DownloadStatus::Complete => "下载完成",
                DownloadStatus::Error(msg) => msg.as_str(),
            };
            ui.label(format!("状态: {}", status_text));

            ui.add_space(15.0);

            // 控制按钮 - 使用克隆的状态来判断
            let status = progress.status.clone();
            let is_complete = status == DownloadStatus::Complete;
            let is_error = matches!(status, DownloadStatus::Error(_));

            ui.horizontal(|ui| {
                match status {
                    DownloadStatus::Active => {
                        if ui.button("暂停").clicked() {
                            self.pause_current_download();
                        }
                    }
                    DownloadStatus::Paused => {
                        if ui.button("继续").clicked() {
                            self.resume_current_download();
                        }
                    }
                    DownloadStatus::Complete => {
                        ui.colored_label(egui::Color32::GREEN, "✓ 下载完成！");
                        
                        // 检查是否需要下载后跳转到安装页面（系统镜像）
                        if self.download_then_install {
                            ui.label("正在跳转到安装页面...");
                            
                            // 获取下载的文件路径
                            if let Some(ref downloaded_path) = self.download_then_install_path {
                                let path = downloaded_path.clone();
                                self.local_image_path = path.clone();
                                // 清理下载状态
                                self.download_then_install = false;
                                self.download_then_install_path = None;
                                self.cleanup_download();
                                // 跳转到安装页面
                                self.current_panel = crate::app::Panel::SystemInstall;
                                // 加载镜像信息
                                self.load_image_volumes();
                            } else {
                                self.download_then_install = false;
                                self.cleanup_download();
                                self.current_panel = crate::app::Panel::SystemInstall;
                            }
                        }
                        // 检查是否需要下载后运行软件
                        else if self.soft_download_then_run {
                            ui.label("正在启动软件...");
                            
                            if let Some(ref run_path) = self.soft_download_then_run_path {
                                let path = run_path.clone();
                                // 清理下载状态
                                self.soft_download_then_run = false;
                                self.soft_download_then_run_path = None;
                                self.cleanup_download();
                                
                                // 运行软件
                                if let Err(e) = std::process::Command::new(&path).spawn() {
                                    log::warn!("启动软件失败: {}", e);
                                }
                                
                                // 返回在线下载页面
                                self.current_panel = crate::app::Panel::OnlineDownload;
                            } else {
                                self.soft_download_then_run = false;
                                self.cleanup_download();
                                self.current_panel = crate::app::Panel::OnlineDownload;
                            }
                        }
                        // 检查是否有待继续的PE操作
                        else if self.pe_download_then_action.is_some() {
                            ui.label("正在准备继续操作...");
                            // 延迟一帧后继续操作，避免状态冲突
                            let action = self.pe_download_then_action.take();
                            self.cleanup_download();
                            
                            match action {
                                Some(crate::app::PeDownloadThenAction::Install) => {
                                    // 继续安装
                                    self.start_installation();
                                }
                                Some(crate::app::PeDownloadThenAction::Backup) => {
                                    // 继续备份
                                    self.start_backup_internal();
                                }
                                None => {
                                    self.current_panel = crate::app::Panel::OnlineDownload;
                                }
                            }
                        } else {
                            if ui.button("返回").clicked() {
                                self.cleanup_download();
                                self.current_panel = crate::app::Panel::OnlineDownload;
                            }
                        }
                    }
                    DownloadStatus::Error(_) => {
                        if ui.button("返回").clicked() {
                            // 先获取待执行操作
                            let action = self.pe_download_then_action.take();
                            self.cleanup_download();
                            // 根据操作类型返回对应页面
                            match action {
                                Some(crate::app::PeDownloadThenAction::Install) => {
                                    self.current_panel = crate::app::Panel::SystemInstall;
                                }
                                Some(crate::app::PeDownloadThenAction::Backup) => {
                                    self.current_panel = crate::app::Panel::SystemBackup;
                                }
                                None => {
                                    self.current_panel = crate::app::Panel::OnlineDownload;
                                }
                            }
                        }
                    }
                    _ => {}
                }

                if !is_complete && !is_error {
                    if ui.button("取消").clicked() {
                        self.cancel_current_download();
                    }
                }
            });
        } else {
            // 显示等待状态或无任务
            if self.current_download.is_some() {
                ui.add_space(15.0);
                ui.label("正在初始化下载...");
                ui.spinner();
            } else {
                ui.label("没有正在进行的下载任务");
                if ui.button("返回").clicked() {
                    self.current_panel = crate::app::Panel::OnlineDownload;
                }
            }
        }
    }

    /// 从channel更新下载进度
    fn update_download_progress(&mut self) {
        if let Some(ref rx) = self.download_progress_rx {
            // 非阻塞接收所有可用的进度更新
            while let Ok(progress) = rx.try_recv() {
                // 保存gid
                if self.download_gid.is_none() && !progress.gid.is_empty() {
                    self.download_gid = Some(progress.gid.clone());
                }
                self.download_progress = Some(progress);
            }
        }
    }

    fn start_download_task(&mut self, url: &str, save_path: &str, filename: Option<&str>) {
        self.current_download_filename = filename.map(|s| s.to_string());
        self.current_download = Some(url.to_string());
        self.download_init_error = None;
        self.download_gid = None;

        // 创建进度通道
        let (progress_tx, progress_rx) = mpsc::channel::<DownloadProgress>();
        self.download_progress_rx = Some(progress_rx);

        // 创建控制通道
        let (cmd_tx, cmd_rx) = mpsc::channel::<DownloadCommand>();
        
        // 清空旧的下载管理器状态
        {
            let mut guard = self.download_manager.lock().unwrap();
            *guard = None;
        }

        // 克隆需要的数据
        let url = url.to_string();
        let save_path = save_path.to_string();
        let filename = filename.map(|s| s.to_string());
        
        // 存储命令发送器
        self.store_download_command_sender(cmd_tx);

        // 在后台线程中执行下载
        std::thread::spawn(move || {
            // 创建新的tokio运行时
            let rt = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt,
                Err(e) => {
                    let _ = progress_tx.send(DownloadProgress {
                        gid: String::new(),
                        completed_length: 0,
                        total_length: 0,
                        download_speed: 0,
                        percentage: 0.0,
                        status: DownloadStatus::Error(format!("创建运行时失败: {}", e)),
                    });
                    return;
                }
            };

            rt.block_on(async move {
                // 初始化 aria2
                let aria2 = match Aria2Manager::start().await {
                    Ok(manager) => manager,
                    Err(e) => {
                        let _ = progress_tx.send(DownloadProgress {
                            gid: String::new(),
                            completed_length: 0,
                            total_length: 0,
                            download_speed: 0,
                            percentage: 0.0,
                            status: DownloadStatus::Error(format!("初始化aria2失败: {}", e)),
                        });
                        return;
                    }
                };

                // 添加下载任务
                let gid = match aria2.add_download(&url, &save_path, filename.as_deref()).await {
                    Ok(gid) => gid,
                    Err(e) => {
                        let _ = progress_tx.send(DownloadProgress {
                            gid: String::new(),
                            completed_length: 0,
                            total_length: 0,
                            download_speed: 0,
                            percentage: 0.0,
                            status: DownloadStatus::Error(format!("添加任务失败: {}", e)),
                        });
                        return;
                    }
                };

                // 定期获取进度并发送，同时监听控制命令
                loop {
                    // 处理控制命令（非阻塞）
                    while let Ok(cmd) = cmd_rx.try_recv() {
                        match cmd {
                            DownloadCommand::Pause => {
                                let _ = aria2.pause(&gid).await;
                            }
                            DownloadCommand::Resume => {
                                let _ = aria2.resume(&gid).await;
                            }
                            DownloadCommand::Cancel => {
                                let _ = aria2.cancel(&gid).await;
                                return;
                            }
                        }
                    }

                    // 获取进度
                    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

                    match aria2.get_status(&gid).await {
                        Ok(progress) => {
                            let is_complete = progress.status == DownloadStatus::Complete;
                            let is_error = matches!(progress.status, DownloadStatus::Error(_));

                            if progress_tx.send(progress).is_err() {
                                break; // 接收端已关闭
                            }

                            if is_complete || is_error {
                                break;
                            }
                        }
                        Err(e) => {
                            let _ = progress_tx.send(DownloadProgress {
                                gid: gid.clone(),
                                completed_length: 0,
                                total_length: 0,
                                download_speed: 0,
                                percentage: 0.0,
                                status: DownloadStatus::Error(format!("获取状态失败: {}", e)),
                            });
                            break;
                        }
                    }
                }
            });
        });
    }

    /// 存储下载命令发送器
    fn store_download_command_sender(&mut self, _sender: mpsc::Sender<DownloadCommand>) {
        // 由于 Rust 的所有权限制，我们使用一个简化的方案：
        // 将命令发送器存储在 thread local 或者通过其他机制
        // 这里我们使用静态变量（需要在实际使用中确保线程安全）
        unsafe {
            DOWNLOAD_CMD_SENDER = Some(_sender);
        }
    }

    fn pause_current_download(&mut self) {
        unsafe {
            if let Some(ref sender) = DOWNLOAD_CMD_SENDER {
                let _ = sender.send(DownloadCommand::Pause);
            }
        }
    }

    fn resume_current_download(&mut self) {
        unsafe {
            if let Some(ref sender) = DOWNLOAD_CMD_SENDER {
                let _ = sender.send(DownloadCommand::Resume);
            }
        }
    }

    fn cancel_current_download(&mut self) {
        unsafe {
            if let Some(ref sender) = DOWNLOAD_CMD_SENDER {
                let _ = sender.send(DownloadCommand::Cancel);
            }
            DOWNLOAD_CMD_SENDER = None;
        }

        // 先获取待执行操作
        let action = self.pe_download_then_action.take();
        let was_download_then_install = self.download_then_install;
        self.cleanup_download();
        
        // 根据操作类型返回对应页面
        if was_download_then_install {
            self.current_panel = crate::app::Panel::OnlineDownload;
        } else {
            match action {
                Some(crate::app::PeDownloadThenAction::Install) => {
                    self.current_panel = crate::app::Panel::SystemInstall;
                }
                Some(crate::app::PeDownloadThenAction::Backup) => {
                    self.current_panel = crate::app::Panel::SystemBackup;
                }
                None => {
                    self.current_panel = crate::app::Panel::OnlineDownload;
                }
            }
        }
    }

    /// 清理下载状态
    fn cleanup_download(&mut self) {
        self.download_progress = None;
        self.current_download = None;
        self.download_gid = None;
        self.download_progress_rx = None;
        self.current_download_filename = None;
        self.pe_download_then_action = None;  // 清除待执行操作
        self.download_then_install = false;   // 清除下载后安装标记
        self.download_then_install_path = None;
        self.soft_download_then_run = false;  // 清除软件下载后运行标记
        self.soft_download_then_run_path = None;
        
        unsafe {
            DOWNLOAD_CMD_SENDER = None;
        }
    }

    fn format_bytes(bytes: u64) -> String {
        const KB: u64 = 1024;
        const MB: u64 = KB * 1024;
        const GB: u64 = MB * 1024;

        if bytes >= GB {
            format!("{:.2} GB", bytes as f64 / GB as f64)
        } else if bytes >= MB {
            format!("{:.2} MB", bytes as f64 / MB as f64)
        } else if bytes >= KB {
            format!("{:.2} KB", bytes as f64 / KB as f64)
        } else {
            format!("{} B", bytes)
        }
    }
}

// 静态变量存储命令发送器（简化实现）
static mut DOWNLOAD_CMD_SENDER: Option<mpsc::Sender<DownloadCommand>> = None;
