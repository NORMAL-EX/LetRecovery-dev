//! 运行 Diskpart 脚本对话框
//!
//! 让用户直接输入一段 Diskpart 脚本（每行一条命令，与交互式 diskpart 等价），
//! 一键执行（`diskpart /s`）并显示输出。适合批量分区/清盘等高级操作。

use egui;
use std::sync::mpsc;

use crate::app::App;
use crate::ui::tools::actions;

impl App {
    /// 渲染「运行 Diskpart 脚本」对话框
    pub fn render_diskpart_script_dialog(&mut self, ui: &mut egui::Ui) {
        if !self.show_diskpart_script_dialog {
            return;
        }

        let mut should_close = false;

        egui::Window::new("运行 Diskpart 脚本")
            .resizable(true)
            .default_width(640.0)
            .default_height(460.0)
            .show(ui.ctx(), |ui| {
                ui.colored_label(
                    egui::Color32::from_rgb(255, 165, 0),
                    "警告：Diskpart 可直接清空磁盘/分区，误操作会丢失数据，请谨慎执行。",
                );
                ui.add_space(6.0);
                ui.label("每行一条命令，与交互式 diskpart 等价。例如：");
                ui.indent("diskpart_example", |ui| {
                    ui.monospace("list disk\nselect disk 0\nlist partition");
                });

                ui.add_space(10.0);
                ui.label("脚本内容：");
                egui::ScrollArea::vertical()
                    .id_salt("diskpart_script_input_area")
                    .max_height(160.0)
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut self.diskpart_script_input)
                                .code_editor()
                                .desired_rows(8)
                                .desired_width(f32::INFINITY)
                                .hint_text("在此输入 diskpart 脚本，每行一条命令"),
                        );
                    });

                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    let can_run = !self.diskpart_script_running
                        && !self.diskpart_script_input.trim().is_empty();
                    if ui.add_enabled(can_run, egui::Button::new("运行")).clicked() {
                        self.start_diskpart_script();
                    }
                    if ui
                        .add_enabled(!self.diskpart_script_running, egui::Button::new("清空"))
                        .clicked()
                    {
                        self.diskpart_script_input.clear();
                        self.diskpart_script_output.clear();
                    }
                    if self.diskpart_script_running {
                        ui.add_space(8.0);
                        ui.spinner();
                        ui.label("正在执行 diskpart...");
                    }
                });

                ui.add_space(10.0);
                ui.separator();
                ui.add_space(8.0);

                ui.label("输出：");
                egui::ScrollArea::vertical()
                    .id_salt("diskpart_script_output_area")
                    .max_height(160.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if self.diskpart_script_output.is_empty() {
                            ui.colored_label(
                                egui::Color32::GRAY,
                                "（执行后在此显示 diskpart 输出）",
                            );
                        } else {
                            // 只读多行框：等宽、可滚动、可选中复制
                            let mut out = self.diskpart_script_output.clone();
                            ui.add(
                                egui::TextEdit::multiline(&mut out)
                                    .code_editor()
                                    .desired_width(f32::INFINITY)
                                    .interactive(false),
                            );
                        }
                    });

                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(!self.diskpart_script_running, egui::Button::new("关闭"))
                        .clicked()
                    {
                        should_close = true;
                    }
                });
            });

        if should_close {
            self.show_diskpart_script_dialog = false;
        }
    }

    /// 在后台线程执行 diskpart 脚本（避免阻塞 UI）
    fn start_diskpart_script(&mut self) {
        if self.diskpart_script_running {
            return;
        }
        let script = self.diskpart_script_input.clone();
        if script.trim().is_empty() {
            return;
        }

        self.diskpart_script_running = true;
        self.diskpart_script_output = "正在执行...".to_string();

        let (tx, rx) = mpsc::channel::<Result<String, String>>();
        self.diskpart_script_rx = Some(rx);

        std::thread::spawn(move || {
            let result = actions::run_diskpart_script(&script);
            let _ = tx.send(result);
        });
    }

    /// 轮询 diskpart 脚本执行状态（在主循环中调用）
    pub fn check_diskpart_script_status(&mut self) {
        if let Some(ref rx) = self.diskpart_script_rx {
            if let Ok(result) = rx.try_recv() {
                self.diskpart_script_output = match result {
                    Ok(text) => {
                        if text.is_empty() {
                            "执行完成（无输出）".to_string()
                        } else {
                            text
                        }
                    }
                    Err(e) => e,
                };
                self.diskpart_script_running = false;
                self.diskpart_script_rx = None;
            }
        }
    }
}
