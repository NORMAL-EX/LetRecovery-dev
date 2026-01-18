use egui;

use crate::app::App;

impl App {
    pub fn show_about(&mut self, ui: &mut egui::Ui) {
        let available_height = ui.available_height();

        egui::ScrollArea::vertical()
            .max_height(available_height)
            .show(ui, |ui| {
                ui.heading("关于 LetRecovery");
                ui.separator();

                ui.add_space(20.0);

                // 版本信息
                ui.horizontal(|ui| {
                    ui.label("版本:");
                    ui.strong("v2026.1.18");
                });

                ui.add_space(15.0);

                // 版权信息
                ui.label("版权:");
                ui.indent("copyright", |ui| {
                    ui.label("\u{00A9} 2026-present Cloud-PE Dev.");
                    ui.label("\u{00A9} 2026-present NORMAL-EX.");
                });

                ui.add_space(15.0);

                // 开源链接
                ui.horizontal(|ui| {
                    ui.label("开源地址:");
                    ui.hyperlink_to(
                        "https://github.com/NORMAL-EX/LetRecovery",
                        "https://github.com/NORMAL-EX/LetRecovery",
                    );
                });

                ui.add_space(10.0);

                // 许可证
                ui.horizontal(|ui| {
                    ui.label("许可证:");
                    ui.strong("PolyForm Noncommercial License 1.0.0");
                });

                ui.add_space(20.0);
                ui.separator();

                // 免费声明
                ui.heading("免费声明");
                ui.add_space(10.0);

                ui.colored_label(
                    egui::Color32::from_rgb(0, 200, 83),
                    "✓ 本软件完全免费，禁止任何形式的倒卖行为！",
                );

                ui.add_space(8.0);

                ui.label("如果您是通过付费渠道获取本软件，您已被骗，请立即举报并申请退款。");

                ui.add_space(15.0);

                // 使用条款
                ui.heading("使用条款");
                ui.add_space(10.0);

                ui.colored_label(egui::Color32::from_rgb(100, 181, 246), "允许：");
                ui.indent("allowed", |ui| {
                    ui.label("• 个人学习、研究和非盈利使用");
                    ui.label("• 修改源代码并用于非盈利用途");
                    ui.label("• 在注明出处的前提下进行非商业性质的分发");
                });

                ui.add_space(10.0);

                ui.colored_label(egui::Color32::from_rgb(239, 83, 80), "禁止：");
                ui.indent("forbidden", |ui| {
                    ui.label("• 将本软件或其源代码用于任何商业/盈利用途");
                    ui.label("• 销售、倒卖本软件或其衍生作品");
                    ui.label("• 将本软件整合到商业产品或服务中");
                    ui.label("• 个人利用本软件或其代码进行盈利活动");
                });

                ui.add_space(20.0);
                ui.separator();

                // 致谢
                ui.heading("致谢");

                ui.add_space(10.0);

                ui.label("• 部分系统镜像及 PE 下载服务由 Cloud-PE 云盘提供");
                ui.label("• 感谢 电脑病毒爱好者 提供 WinPE");

                ui.add_space(30.0);
                ui.separator();

                // 说明
                ui.add_space(10.0);
                ui.colored_label(
                    egui::Color32::GRAY,
                    "LetRecovery 是一款免费开源的 Windows 系统重装工具，",
                );
                ui.colored_label(
                    egui::Color32::GRAY,
                    "支持本地镜像安装、在线下载安装、系统备份等功能。",
                );

                ui.add_space(20.0);
            });
    }
}