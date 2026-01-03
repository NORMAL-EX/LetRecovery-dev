use anyhow::Result;
use std::io::Read;
use std::process::Stdio;
use std::sync::mpsc::Sender;

use crate::utils::command::new_command;
use crate::utils::encoding::gbk_to_utf8;
use crate::utils::path::get_bin_dir;

/// DISM 操作进度
#[derive(Debug, Clone)]
pub struct DismProgress {
    pub percentage: u8,
    pub status: String,
}

/// 镜像分卷信息
#[derive(Debug, Clone)]
pub struct ImageInfo {
    pub index: u32,
    pub name: String,
    pub size_bytes: u64,
}

pub struct Dism {
    dism_path: String,
}

impl Dism {
    pub fn new() -> Self {
        let bin_dir = get_bin_dir();
        Self {
            dism_path: bin_dir
                .join("dism")
                .join("dism.exe")
                .to_string_lossy()
                .to_string(),
        }
    }

    /// 检查 DISM 是否可用
    pub fn is_available(&self) -> bool {
        std::path::Path::new(&self.dism_path).exists()
    }

    /// 应用系统镜像 (WIM/ESD)
    pub fn apply_image(
        &self,
        image_file: &str,
        apply_dir: &str,
        index: u32,
        progress_tx: Option<Sender<DismProgress>>,
    ) -> Result<()> {
        log::info!(
            "开始释放镜像: {} -> {} (Index: {})",
            image_file,
            apply_dir,
            index
        );

        let args = [
            "/Apply-Image",
            &format!("/ImageFile:{}", image_file),
            &format!("/ApplyDir:{}", apply_dir),
            &format!("/Index:{}", index),
        ];

        self.run_with_progress(&args, progress_tx)
    }

    /// 捕获系统镜像 (备份)
    pub fn capture_image(
        &self,
        image_file: &str,
        capture_dir: &str,
        name: &str,
        description: &str,
        progress_tx: Option<Sender<DismProgress>>,
    ) -> Result<()> {
        log::info!("开始备份镜像: {} -> {}", capture_dir, image_file);

        let args = [
            "/Capture-Image",
            &format!("/ImageFile:{}", image_file),
            &format!("/CaptureDir:{}", capture_dir),
            &format!("/Name:{}", name),
            &format!("/Description:{}", description),
            "/Compress:max",
        ];

        self.run_with_progress(&args, progress_tx)
    }

    /// 增量备份镜像
    pub fn append_image(
        &self,
        image_file: &str,
        capture_dir: &str,
        name: &str,
        description: &str,
        progress_tx: Option<Sender<DismProgress>>,
    ) -> Result<()> {
        log::info!("开始增量备份: {} -> {}", capture_dir, image_file);

        let args = [
            "/Append-Image",
            &format!("/ImageFile:{}", image_file),
            &format!("/CaptureDir:{}", capture_dir),
            &format!("/Name:{}", name),
            &format!("/Description:{}", description),
        ];

        self.run_with_progress(&args, progress_tx)
    }

    /// 导入驱动到离线系统
    pub fn add_drivers_offline(&self, image_path: &str, driver_path: &str) -> Result<()> {
        log::info!("导入驱动: {} -> {}", driver_path, image_path);

        let output = new_command(&self.dism_path)
            .args([
                &format!("/image:{}", image_path),
                "/add-driver",
                "/forceunsigned",
                &format!("/driver:{}", driver_path),
                "/recurse",
            ])
            .output()?;

        if !output.status.success() {
            let stderr = gbk_to_utf8(&output.stderr);
            anyhow::bail!("导入驱动失败: {}", stderr);
        }
        Ok(())
    }

    /// 获取 WIM/ESD 镜像信息（所有分卷）
    pub fn get_image_info(&self, image_file: &str) -> Result<Vec<ImageInfo>> {
        let output = new_command(&self.dism_path)
            .args(["/get-imageinfo", &format!("/imagefile:{}", image_file)])
            .output()?;

        let stdout = gbk_to_utf8(&output.stdout);
        Self::parse_image_info(&stdout)
    }

    fn parse_image_info(output: &str) -> Result<Vec<ImageInfo>> {
        let mut images = Vec::new();
        let mut current_index = 0u32;
        let mut current_name = String::new();
        let mut current_size = 0u64;

        for line in output.lines() {
            let line = line.trim();

            if line.starts_with("索引") || line.starts_with("Index") {
                if current_index > 0 {
                    images.push(ImageInfo {
                        index: current_index,
                        name: current_name.clone(),
                        size_bytes: current_size,
                    });
                }
                if let Some(num) = line.split(':').nth(1) {
                    current_index = num.trim().parse().unwrap_or(0);
                }
                current_name.clear();
                current_size = 0;
            } else if line.contains("名称") || line.contains("Name") {
                if let Some(name) = line.split(':').nth(1) {
                    current_name = name.trim().to_string();
                }
            } else if line.contains("大小") || line.contains("Size") {
                if let Some(size_str) = line.split(':').nth(1) {
                    let size_str = size_str
                        .replace(",", "")
                        .replace(" ", "")
                        .replace("字节", "")
                        .replace("bytes", "");
                    current_size = size_str.trim().parse().unwrap_or(0);
                }
            }
        }

        if current_index > 0 {
            images.push(ImageInfo {
                index: current_index,
                name: current_name,
                size_bytes: current_size,
            });
        }

        Ok(images)
    }

    /// 执行 DISM 命令并实时获取进度
    fn run_with_progress(
        &self,
        args: &[&str],
        progress_tx: Option<Sender<DismProgress>>,
    ) -> Result<()> {
        log::info!("DISM 命令: {} {:?}", &self.dism_path, args);

        let mut child = new_command(&self.dism_path)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        if let Some(mut stdout) = child.stdout.take() {
            let mut buffer = [0u8; 1024];
            let mut line_buffer = Vec::new();

            loop {
                match stdout.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(n) => {
                        for &byte in &buffer[..n] {
                            if byte == b'\r' || byte == b'\n' {
                                if !line_buffer.is_empty() {
                                    let line = gbk_to_utf8(&line_buffer);
                                    log::debug!("DISM: {}", line);

                                    if let Some(progress) = Self::parse_progress_line(&line) {
                                        if let Some(ref tx) = progress_tx {
                                            let _ = tx.send(progress);
                                        }
                                    }
                                    line_buffer.clear();
                                }
                            } else {
                                line_buffer.push(byte);
                            }
                        }
                    }
                    Err(e) => {
                        if e.kind() != std::io::ErrorKind::Interrupted {
                            break;
                        }
                    }
                }
            }

            // 处理剩余的数据
            if !line_buffer.is_empty() {
                let line = gbk_to_utf8(&line_buffer);
                if let Some(progress) = Self::parse_progress_line(&line) {
                    if let Some(ref tx) = progress_tx {
                        let _ = tx.send(progress);
                    }
                }
            }
        }

        // 读取 stderr
        if let Some(mut stderr) = child.stderr.take() {
            let mut err_output = Vec::new();
            let _ = stderr.read_to_end(&mut err_output);
            if !err_output.is_empty() {
                let err_str = gbk_to_utf8(&err_output);
                log::debug!("DISM STDERR: {}", err_str);
            }
        }

        let status = child.wait()?;
        log::info!("DISM 命令结束，状态: {:?}", status);

        if !status.success() {
            anyhow::bail!("DISM 命令执行失败");
        }
        Ok(())
    }

    /// 解析 DISM 进度输出
    fn parse_progress_line(line: &str) -> Option<DismProgress> {
        let mut percentage: Option<u8> = None;

        // 查找 xx.x% 或 xx% 格式
        let line_chars: Vec<char> = line.chars().collect();
        for i in 0..line_chars.len() {
            if line_chars[i] == '%' && i > 0 {
                let mut num_str = String::new();
                let mut j = i as i32 - 1;

                while j >= 0 {
                    let c = line_chars[j as usize];
                    if c.is_ascii_digit() || c == '.' {
                        num_str.insert(0, c);
                        j -= 1;
                    } else if c == ' ' && num_str.is_empty() {
                        j -= 1;
                    } else {
                        break;
                    }
                }

                if !num_str.is_empty() {
                    if let Some(dot_pos) = num_str.find('.') {
                        num_str = num_str[..dot_pos].to_string();
                    }
                    if let Ok(p) = num_str.parse::<u8>() {
                        percentage = Some(p);
                        break;
                    }
                }
            }
        }

        percentage.map(|p| DismProgress {
            percentage: p,
            status: format!("{}%", p),
        })
    }
}

impl Default for Dism {
    fn default() -> Self {
        Self::new()
    }
}
