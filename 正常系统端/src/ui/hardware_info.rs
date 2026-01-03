use egui;

use crate::app::App;
use crate::core::hardware_info::{format_bytes, HardwareInfo};

impl App {
    pub fn show_hardware_info(&mut self, ui: &mut egui::Ui) {
        ui.heading("系统与硬件信息");
        ui.separator();

        // PE 环境提示
        if let Some(info) = &self.system_info {
            if info.is_pe_environment {
                ui.colored_label(
                    egui::Color32::from_rgb(100, 200, 255),
                    "🖥 当前运行在 PE 环境中",
                );
                ui.add_space(5.0);
            }
        }

        egui::ScrollArea::vertical()
            .id_salt("hardware_scroll")
            .show(ui, |ui| {
                // 让内容占满整个宽度，滚动条自然就在右边
                ui.set_min_width(ui.available_width());
                
                if let Some(hw_info) = &self.hardware_info.clone() {
                    // 操作系统信息
                    egui::CollapsingHeader::new("🖥 操作系统")
                        .default_open(true)
                        .show(ui, |ui| {
                            egui::Grid::new("os_grid")
                                .num_columns(2)
                                .spacing([40.0, 4.0])
                                .show(ui, |ui| {
                                    if !hw_info.os.name.is_empty() {
                                        ui.label("系统名称:");
                                        ui.label(&hw_info.os.name);
                                        ui.end_row();
                                    }
                                    
                                    if !hw_info.os.version.is_empty() {
                                        ui.label("版本:");
                                        ui.label(&hw_info.os.version);
                                        ui.end_row();
                                    }
                                    
                                    if !hw_info.os.build_number.is_empty() {
                                        ui.label("内部版本:");
                                        ui.label(&hw_info.os.build_number);
                                        ui.end_row();
                                    }
                                    
                                    if !hw_info.os.architecture.is_empty() {
                                        ui.label("系统类型:");
                                        ui.label(&hw_info.os.architecture);
                                        ui.end_row();
                                    }

                                    // 从 system_info 获取启动模式等信息
                                    if let Some(sys_info) = &self.system_info {
                                        ui.label("启动模式:");
                                        ui.label(format!("{}", sys_info.boot_mode));
                                        ui.end_row();

                                        ui.label("TPM 状态:");
                                        ui.label(if sys_info.tpm_enabled {
                                            format!("已启用 (版本 {})", sys_info.tpm_version)
                                        } else {
                                            "未启用/未检测到".to_string()
                                        });
                                        ui.end_row();

                                        ui.label("安全启动:");
                                        ui.label(if sys_info.secure_boot { "已开启" } else { "已关闭/未检测到" });
                                        ui.end_row();

                                        ui.label("网络状态:");
                                        ui.label(if sys_info.is_online { "已联网" } else { "未联网" });
                                        ui.end_row();
                                    }
                                    
                                    if !hw_info.os.install_date.is_empty() {
                                        ui.label("安装日期:");
                                        ui.label(&hw_info.os.install_date);
                                        ui.end_row();
                                    }
                                    
                                    if !hw_info.os.registered_owner.is_empty() {
                                        ui.label("注册用户:");
                                        ui.label(&hw_info.os.registered_owner);
                                        ui.end_row();
                                    }
                                    
                                    if !hw_info.os.product_id.is_empty() {
                                        ui.label("产品 ID:");
                                        ui.label(&hw_info.os.product_id);
                                        ui.end_row();
                                    }
                                });
                        });

                    // 计算机信息
                    if !hw_info.computer_name.is_empty() || !hw_info.computer_manufacturer.is_empty() {
                        egui::CollapsingHeader::new("💻 计算机")
                            .default_open(true)
                            .show(ui, |ui| {
                                egui::Grid::new("computer_grid")
                                    .num_columns(2)
                                    .spacing([40.0, 4.0])
                                    .show(ui, |ui| {
                                        if !hw_info.computer_name.is_empty() {
                                            ui.label("计算机名:");
                                            ui.label(&hw_info.computer_name);
                                            ui.end_row();
                                        }
                                        
                                        if !hw_info.computer_manufacturer.is_empty() {
                                            ui.label("制造商:");
                                            ui.label(&hw_info.computer_manufacturer);
                                            ui.end_row();
                                        }
                                        
                                        if !hw_info.computer_model.is_empty() {
                                            ui.label("型号:");
                                            ui.label(&hw_info.computer_model);
                                            ui.end_row();
                                        }
                                    });
                            });
                    }

                    // CPU 信息
                    egui::CollapsingHeader::new("🔲 处理器 (CPU)")
                        .default_open(true)
                        .show(ui, |ui| {
                            egui::Grid::new("cpu_grid")
                                .num_columns(2)
                                .spacing([40.0, 4.0])
                                .show(ui, |ui| {
                                    ui.label("名称:");
                                    ui.label(&hw_info.cpu.name);
                                    ui.end_row();
                                    
                                    if !hw_info.cpu.manufacturer.is_empty() {
                                        ui.label("制造商:");
                                        ui.label(&hw_info.cpu.manufacturer);
                                        ui.end_row();
                                    }
                                    
                                    ui.label("架构:");
                                    ui.label(&hw_info.cpu.architecture);
                                    ui.end_row();
                                    
                                    ui.label("核心/线程:");
                                    ui.label(format!("{} 核心 / {} 线程", 
                                        hw_info.cpu.cores, 
                                        hw_info.cpu.logical_processors));
                                    ui.end_row();
                                    
                                    if hw_info.cpu.max_clock_speed > 0 {
                                        ui.label("频率:");
                                        ui.label(format!("{:.2} GHz", 
                                            hw_info.cpu.max_clock_speed as f64 / 1000.0));
                                        ui.end_row();
                                    }
                                    
                                    if hw_info.cpu.l2_cache_size > 0 {
                                        ui.label("L2 缓存:");
                                        ui.label(format!("{} KB", hw_info.cpu.l2_cache_size));
                                        ui.end_row();
                                    }
                                    
                                    if hw_info.cpu.l3_cache_size > 0 {
                                        ui.label("L3 缓存:");
                                        ui.label(format!("{:.1} MB", 
                                            hw_info.cpu.l3_cache_size as f64 / 1024.0));
                                        ui.end_row();
                                    }
                                });
                        });

                    // 内存信息
                    egui::CollapsingHeader::new("📊 内存 (RAM)")
                        .default_open(true)
                        .show(ui, |ui| {
                            egui::Grid::new("memory_grid")
                                .num_columns(2)
                                .spacing([40.0, 4.0])
                                .show(ui, |ui| {
                                    ui.label("物理内存:");
                                    ui.label(format_bytes(hw_info.memory.total_physical));
                                    ui.end_row();
                                    
                                    ui.label("可用内存:");
                                    ui.label(format_bytes(hw_info.memory.available_physical));
                                    ui.end_row();
                                    
                                    ui.label("使用率:");
                                    ui.label(format!("{}%", hw_info.memory.memory_load));
                                    ui.end_row();
                                });
                        });

                    // 主板信息
                    egui::CollapsingHeader::new("🔧 主板")
                        .default_open(true)
                        .show(ui, |ui| {
                            egui::Grid::new("motherboard_grid")
                                .num_columns(2)
                                .spacing([40.0, 4.0])
                                .show(ui, |ui| {
                                    if !hw_info.motherboard.manufacturer.is_empty() {
                                        ui.label("制造商:");
                                        ui.label(&hw_info.motherboard.manufacturer);
                                        ui.end_row();
                                    }
                                    
                                    if !hw_info.motherboard.product.is_empty() {
                                        ui.label("产品:");
                                        ui.label(&hw_info.motherboard.product);
                                        ui.end_row();
                                    }
                                    
                                    if !hw_info.motherboard.version.is_empty() 
                                        && hw_info.motherboard.version != "Default string" {
                                        ui.label("版本:");
                                        ui.label(&hw_info.motherboard.version);
                                        ui.end_row();
                                    }
                                });
                            
                            // BIOS 信息
                            ui.add_space(8.0);
                            ui.label("BIOS:");
                            egui::Grid::new("bios_grid")
                                .num_columns(2)
                                .spacing([40.0, 4.0])
                                .show(ui, |ui| {
                                    if !hw_info.bios.manufacturer.is_empty() {
                                        ui.label("制造商:");
                                        ui.label(&hw_info.bios.manufacturer);
                                        ui.end_row();
                                    }
                                    
                                    if !hw_info.bios.smbios_version.is_empty() {
                                        ui.label("版本:");
                                        ui.label(&hw_info.bios.smbios_version);
                                        ui.end_row();
                                    }
                                    
                                    if !hw_info.bios.release_date.is_empty() {
                                        ui.label("日期:");
                                        ui.label(&hw_info.bios.release_date);
                                        ui.end_row();
                                    }
                                });
                        });

                    // 硬盘信息
                    if !hw_info.disks.is_empty() {
                        egui::CollapsingHeader::new("💾 硬盘")
                            .default_open(true)
                            .show(ui, |ui| {
                                for (i, disk) in hw_info.disks.iter().enumerate() {
                                    if hw_info.disks.len() > 1 {
                                        ui.label(format!("硬盘 {}:", i + 1));
                                    }
                                    egui::Grid::new(format!("disk_grid_{}", i))
                                        .num_columns(2)
                                        .spacing([40.0, 4.0])
                                        .show(ui, |ui| {
                                            if !disk.model.is_empty() {
                                                ui.label("型号:");
                                                ui.label(&disk.model);
                                                ui.end_row();
                                            }
                                            
                                            if !disk.interface_type.is_empty() {
                                                ui.label("接口:");
                                                ui.label(&disk.interface_type);
                                                ui.end_row();
                                            }
                                            
                                            if !disk.serial_number.is_empty() {
                                                ui.label("序列号:");
                                                ui.label(&disk.serial_number);
                                                ui.end_row();
                                            }
                                            
                                            if !disk.firmware_revision.is_empty() {
                                                ui.label("固件:");
                                                ui.label(&disk.firmware_revision);
                                                ui.end_row();
                                            }
                                        });
                                    if i < hw_info.disks.len() - 1 {
                                        ui.add_space(5.0);
                                    }
                                }
                            });
                    }

                    // 显卡信息
                    if !hw_info.gpus.is_empty() {
                        egui::CollapsingHeader::new("🎮 显卡 (GPU)")
                            .default_open(true)
                            .show(ui, |ui| {
                                for (i, gpu) in hw_info.gpus.iter().enumerate() {
                                    if hw_info.gpus.len() > 1 {
                                        ui.label(format!("显卡 {}:", i + 1));
                                    }
                                    egui::Grid::new(format!("gpu_grid_{}", i))
                                        .num_columns(2)
                                        .spacing([40.0, 4.0])
                                        .show(ui, |ui| {
                                            if !gpu.name.is_empty() {
                                                ui.label("名称:");
                                                ui.label(&gpu.name);
                                                ui.end_row();
                                            }
                                            
                                            if !gpu.current_resolution.is_empty() && gpu.current_resolution != "0x0" {
                                                ui.label("分辨率:");
                                                ui.label(format!("{} @ {}Hz", 
                                                    gpu.current_resolution, 
                                                    gpu.refresh_rate));
                                                ui.end_row();
                                            }
                                        });
                                    if i < hw_info.gpus.len() - 1 {
                                        ui.add_space(5.0);
                                    }
                                }
                            });
                    }

                    // 磁盘分区信息
                    egui::CollapsingHeader::new("📁 磁盘分区")
                        .default_open(true)
                        .show(ui, |ui| {
                            let is_pe = self.system_info.as_ref().map(|s| s.is_pe_environment).unwrap_or(false);
                            
                            egui::Grid::new("partition_grid")
                                .striped(true)
                                .min_col_width(60.0)
                                .show(ui, |ui| {
                                    ui.label("分区");
                                    ui.label("卷标");
                                    ui.label("总容量");
                                    ui.label("可用");
                                    ui.label("使用率");
                                    ui.end_row();

                                    for partition in &self.partitions {
                                        let used = partition.total_size_mb - partition.free_size_mb;
                                        let usage = if partition.total_size_mb > 0 {
                                            (used as f64 / partition.total_size_mb as f64) * 100.0
                                        } else {
                                            0.0
                                        };

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
                                        ui.label(format!("{:.0}%", usage));
                                        ui.end_row();
                                    }
                                });
                        });

                } else {
                    ui.spinner();
                    ui.label("正在加载硬件信息...");
                }
            });

        ui.add_space(10.0);
        
        // 刷新按钮
        if ui.button("刷新信息").clicked() {
            self.refresh_all_info();
        }
    }

    fn refresh_all_info(&mut self) {
        // 刷新系统信息
        if let Ok(info) = crate::core::system_info::SystemInfo::collect() {
            self.system_info = Some(info);
        }

        // 刷新硬件信息
        if let Ok(info) = crate::core::hardware_info::HardwareInfo::collect() {
            self.hardware_info = Some(info);
        }

        // 刷新分区信息
        if let Ok(partitions) = crate::core::disk::DiskManager::get_partitions() {
            self.partitions = partitions;
        }
    }
}
