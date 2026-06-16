//! 离线密码重置对话框
//!
//! 对**离线的** Windows 安装（如另一块盘/分区上的系统、整盘备份还原后的系统）：
//! 清除指定本地账户的密码（等效空密码）并启用被禁用的账户。
//! 复用共享库 `lr_core::sam::clear_account_password`（含强制备份、成功后删除备份等安全措施）。
//!
//! 需要管理员权限；目标分区须存在 `\Windows\System32\config\SAM`。

use egui;
use std::sync::mpsc;

use crate::app::App;

impl App {
    /// 渲染离线密码重置对话框
    pub fn render_password_reset_dialog(&mut self, ui: &mut egui::Ui) {
        if !self.show_password_reset_dialog {
            return;
        }

        let mut should_close = false;
        let mut do_reset = false;

        egui::Window::new("🔑 离线密码重置")
            .resizable(true)
            .default_width(560.0)
            .default_height(280.0)
            .show(ui.ctx(), |ui| {
                ui.label("清除离线 Windows 本地账户的密码（等效空密码），并启用被禁用的账户。");
                ui.colored_label(
                    egui::Color32::from_rgb(255, 165, 0),
                    "⚠ 仅用于自己的系统/已授权场景；将修改目标系统的 SAM（操作前自动备份）。",
                );
                ui.add_space(10.0);

                ui.horizontal(|ui| {
                    ui.label("目标系统盘符:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.password_reset_partition)
                            .hint_text("如 D: （离线 Windows 所在分区）")
                            .desired_width(140.0),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label("用户名:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.password_reset_username)
                            .hint_text("要重置密码的本地账户名，如 Administrator")
                            .desired_width(280.0),
                    );
                });

                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    let can_reset = !self.password_reset_loading
                        && !self.password_reset_partition.trim().is_empty()
                        && !self.password_reset_username.trim().is_empty();
                    if ui.add_enabled(can_reset, egui::Button::new("重置密码")).clicked() {
                        do_reset = true;
                    }
                    if self.password_reset_loading {
                        ui.add_space(10.0);
                        ui.spinner();
                        ui.label("正在处理...");
                    }
                });

                if !self.password_reset_message.is_empty() {
                    ui.add_space(10.0);
                    ui.separator();
                    let color = if self.password_reset_message.starts_with('✓') {
                        egui::Color32::from_rgb(0, 200, 0)
                    } else if self.password_reset_message.starts_with('✗') {
                        egui::Color32::from_rgb(255, 80, 80)
                    } else {
                        egui::Color32::GRAY
                    };
                    ui.colored_label(color, &self.password_reset_message);
                }

                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    if ui.button("关闭").clicked() {
                        should_close = true;
                    }
                });
            });

        if do_reset {
            self.start_password_reset();
        }
        if should_close {
            self.show_password_reset_dialog = false;
        }
    }

    /// 启动离线密码重置（后台线程）
    fn start_password_reset(&mut self) {
        if self.password_reset_loading {
            return;
        }
        // 规范化盘符为 "X:"
        let raw = self.password_reset_partition.trim();
        let letter = raw.chars().next().unwrap_or(' ');
        if !letter.is_ascii_alphabetic() {
            self.password_reset_message = "✗ 盘符无效，请输入如 D:".to_string();
            return;
        }
        let partition = format!("{}:", letter.to_ascii_uppercase());
        let username = self.password_reset_username.trim().to_string();
        if username.is_empty() {
            self.password_reset_message = "✗ 请输入用户名".to_string();
            return;
        }

        // 预检查 SAM 是否存在，给出更友好的提示
        let sam = format!("{}\\Windows\\System32\\config\\SAM", partition);
        if !std::path::Path::new(&sam).exists() {
            self.password_reset_message =
                format!("✗ 未在 {} 找到 Windows（缺少 {}）", partition, sam);
            return;
        }

        self.password_reset_loading = true;
        self.password_reset_message = "正在重置密码...".to_string();

        let (tx, rx) = mpsc::channel::<Result<bool, String>>();
        self.password_reset_rx = Some(rx);

        std::thread::spawn(move || {
            let result =
                lr_core::sam::clear_account_password(&partition, &username).map_err(|e| e.to_string());
            let _ = tx.send(result);
        });
    }

    /// 轮询离线密码重置状态（在主循环中调用）
    pub fn check_password_reset_status(&mut self) {
        if let Some(ref rx) = self.password_reset_rx {
            if let Ok(result) = rx.try_recv() {
                self.password_reset_loading = false;
                self.password_reset_rx = None;
                self.password_reset_message = match result {
                    Ok(true) => "✓ 已重置该账户密码（可空密码登录），并已启用账户".to_string(),
                    Ok(false) => {
                        "✗ 未找到匹配的账户（请核对用户名），SAM 未改动".to_string()
                    }
                    Err(e) => format!("✗ 失败：{}", e),
                };
            }
        }
    }
}
