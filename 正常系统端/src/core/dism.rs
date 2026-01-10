use anyhow::Result;
use std::process::Stdio;
use std::sync::mpsc::Sender;

use crate::utils::cmd::create_command;
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
    /// 安装类型，用于过滤 WindowsPE 等非系统镜像
    /// 值如: "Client", "WindowsPE", "Server" 等
    pub installation_type: String,
}

pub struct Dism {
    dism_path: String,
    is_pe: bool,
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
            is_pe: crate::core::system_info::SystemInfo::check_pe_environment(),
        }
    }

    /// 检查是否在 PE 环境
    pub fn is_pe_environment(&self) -> bool {
        self.is_pe
    }

    /// 应用系统镜像 (WIM/ESD)
    pub fn apply_image(
        &self,
        image_file: &str,
        apply_dir: &str,
        index: u32,
        progress_tx: Option<Sender<DismProgress>>,
    ) -> Result<()> {
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
        let args = [
            "/Append-Image",
            &format!("/ImageFile:{}", image_file),
            &format!("/CaptureDir:{}", capture_dir),
            &format!("/Name:{}", name),
            &format!("/Description:{}", description),
        ];

        self.run_with_progress(&args, progress_tx)
    }

    /// 导出驱动 - 自动检测环境
    /// 在PE环境下，需要指定源系统路径 (如 "C:")
    /// 在正常环境下，使用 /online
    pub fn export_drivers(&self, destination: &str) -> Result<()> {
        std::fs::create_dir_all(destination)?;

        if self.is_pe {
            anyhow::bail!("PE环境下无法导出当前系统驱动，请使用 export_drivers_from_system 并指定目标系统分区");
        }

        let output = create_command(&self.dism_path)
            .args([
                "/online",
                "/export-driver",
                &format!("/destination:{}", destination),
            ])
            .output()?;

        if !output.status.success() {
            let stderr = gbk_to_utf8(&output.stderr);
            anyhow::bail!("Failed to export drivers: {}", stderr);
        }
        Ok(())
    }

    /// 从指定系统分区导出驱动 (PE环境下使用)
    pub fn export_drivers_from_system(&self, system_partition: &str, destination: &str) -> Result<()> {
        std::fs::create_dir_all(destination)?;

        // 使用离线方式导出驱动
        let image_path = format!("{}\\", system_partition);
        
        let output = create_command(&self.dism_path)
            .args([
                &format!("/image:{}", image_path),
                "/export-driver",
                &format!("/destination:{}", destination),
            ])
            .output()?;

        if !output.status.success() {
            let stderr = gbk_to_utf8(&output.stderr);
            let stdout = gbk_to_utf8(&output.stdout);
            anyhow::bail!("Failed to export drivers from {}: {} {}", system_partition, stderr, stdout);
        }
        Ok(())
    }

    /// 导入驱动 - 自动检测环境
    /// 在PE环境下，自动转为离线操作
    pub fn add_drivers(&self, target_path: &str, driver_path: &str) -> Result<()> {
        if self.is_pe {
            // PE环境下使用离线方式
            self.add_drivers_offline(target_path, driver_path)
        } else {
            // 正常环境下使用在线方式
            self.add_drivers_online(driver_path)
        }
    }

    /// 导入驱动到在线系统 (仅在正常Windows环境下可用)
    pub fn add_drivers_online(&self, driver_path: &str) -> Result<()> {
        if self.is_pe {
            anyhow::bail!("PE环境下无法使用在线方式添加驱动，请使用 add_drivers_offline");
        }

        let output = create_command(&self.dism_path)
            .args([
                "/online",
                "/add-driver",
                "/forceunsigned",
                &format!("/driver:{}", driver_path),
                "/recurse",
            ])
            .output()?;

        if !output.status.success() {
            let stderr = gbk_to_utf8(&output.stderr);
            anyhow::bail!("Failed to add drivers: {}", stderr);
        }
        Ok(())
    }

    /// 导入驱动到离线系统 (PE和正常环境都可用)
    pub fn add_drivers_offline(&self, image_path: &str, driver_path: &str) -> Result<()> {
        let output = create_command(&self.dism_path)
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
            anyhow::bail!("Failed to add drivers to offline image: {}", stderr);
        }
        Ok(())
    }

    /// 获取 WIM/ESD 镜像信息（所有分卷）
    /// 首先尝试直接从 WIM XML 元数据读取，如果失败则使用 DISM 命令
    pub fn get_image_info(&self, image_file: &str) -> Result<Vec<ImageInfo>> {
        // 首先尝试直接解析 WIM XML 元数据（更快更可靠）
        if let Ok(images) = Self::parse_wim_xml_metadata(image_file) {
            if !images.is_empty() {
                println!("[DISM] 从 WIM XML 元数据成功解析出 {} 个镜像", images.len());
                return Ok(images);
            }
        }

        // 如果 XML 解析失败，使用 DISM 命令
        println!("[DISM] XML 解析失败，使用 DISM 命令获取镜像信息");
        self.get_image_info_via_dism(image_file)
    }

    /// 使用 DISM 命令获取镜像信息
    fn get_image_info_via_dism(&self, image_file: &str) -> Result<Vec<ImageInfo>> {
        // 首先获取所有索引的基本信息
        let output = create_command(&self.dism_path)
            .args(["/get-imageinfo", &format!("/imagefile:{}", image_file)])
            .output()?;

        let stdout = gbk_to_utf8(&output.stdout);
        let mut images = Self::parse_basic_image_info(&stdout)?;

        // 对每个索引获取详细信息（包括 Installation Type）
        for image in &mut images {
            if let Ok(detail_output) = create_command(&self.dism_path)
                .args([
                    "/get-imageinfo",
                    &format!("/imagefile:{}", image_file),
                    &format!("/index:{}", image.index),
                ])
                .output()
            {
                let detail_stdout = gbk_to_utf8(&detail_output.stdout);
                // 解析详细信息中的 Installation Type
                for line in detail_stdout.lines() {
                    let line = line.trim();
                    if line.contains("安装类型") || line.contains("Installation Type") {
                        if let Some(install_type) = line.split(':').nth(1) {
                            image.installation_type = install_type.trim().to_string();
                            println!("[DISM] 索引 {} 的安装类型: {}", image.index, image.installation_type);
                            break;
                        }
                    }
                }
            }
        }

        Ok(images)
    }

    /// 直接解析 WIM 文件的 XML 元数据
    /// WIM 文件头部包含完整的 XML 元数据，包括所有镜像的 INSTALLATIONTYPE
    fn parse_wim_xml_metadata(image_file: &str) -> Result<Vec<ImageInfo>> {
        use std::fs::File;
        use std::io::{Read, Seek, SeekFrom};

        println!("[DISM] 尝试直接解析 WIM XML 元数据: {}", image_file);

        let mut file = File::open(image_file)?;
        
        // 读取 WIM 文件头（208 字节）
        let mut header = [0u8; 208];
        file.read_exact(&mut header)?;

        // 验证 WIM 签名 "MSWIM\0\0\0"
        let signature = &header[0..8];
        if signature != b"MSWIM\0\0\0" {
            anyhow::bail!("不是有效的 WIM 文件");
        }

        // 从头部读取 XML 数据的偏移量和大小
        // 偏移量在 header[48..56] (8字节, little-endian)
        // 大小在 header[56..64] (8字节, little-endian)
        let xml_offset = u64::from_le_bytes(header[48..56].try_into().unwrap());
        let xml_size = u64::from_le_bytes(header[56..64].try_into().unwrap());

        if xml_offset == 0 || xml_size == 0 || xml_size > 100_000_000 {
            anyhow::bail!("XML 元数据位置无效");
        }

        println!("[DISM] XML 偏移: {}, 大小: {}", xml_offset, xml_size);

        // 读取 XML 数据
        file.seek(SeekFrom::Start(xml_offset))?;
        let mut xml_data = vec![0u8; xml_size as usize];
        file.read_exact(&mut xml_data)?;

        // XML 数据是 UTF-16LE 编码
        let xml_string = Self::decode_utf16le(&xml_data)?;
        
        // 解析 XML
        Self::parse_wim_xml(&xml_string)
    }

    /// 将 UTF-16LE 编码的字节数组转换为 UTF-8 字符串
    fn decode_utf16le(data: &[u8]) -> Result<String> {
        if data.len() < 2 {
            anyhow::bail!("数据太短");
        }

        // 检查并跳过 BOM (0xFF 0xFE)
        let start = if data.len() >= 2 && data[0] == 0xFF && data[1] == 0xFE {
            2
        } else {
            0
        };

        // 确保数据长度是偶数
        let len = (data.len() - start) / 2;
        let mut utf16_data = Vec::with_capacity(len);
        
        for i in 0..len {
            let offset = start + i * 2;
            if offset + 1 < data.len() {
                let code_unit = u16::from_le_bytes([data[offset], data[offset + 1]]);
                utf16_data.push(code_unit);
            }
        }

        // 去除尾部的空字符
        while utf16_data.last() == Some(&0) {
            utf16_data.pop();
        }

        String::from_utf16(&utf16_data)
            .map_err(|e| anyhow::anyhow!("UTF-16 解码失败: {}", e))
    }

    /// 解析 WIM XML 元数据字符串
    fn parse_wim_xml(xml: &str) -> Result<Vec<ImageInfo>> {
        let mut images = Vec::new();

        // 查找所有 <IMAGE INDEX="X"> ... </IMAGE> 块
        let mut pos = 0;
        while let Some(start) = xml[pos..].find("<IMAGE INDEX=\"") {
            let abs_start = pos + start;
            
            // 找到索引值
            let index_start = abs_start + 14; // "<IMAGE INDEX=\"" 的长度
            if let Some(index_end) = xml[index_start..].find('"') {
                let index_str = &xml[index_start..index_start + index_end];
                let index: u32 = index_str.parse().unwrap_or(0);

                // 找到对应的 </IMAGE>
                if let Some(image_end) = xml[abs_start..].find("</IMAGE>") {
                    let image_block = &xml[abs_start..abs_start + image_end + 8];
                    
                    // 从这个块中提取信息
                    let name = Self::extract_xml_tag(image_block, "NAME")
                        .or_else(|| Self::extract_xml_tag(image_block, "DISPLAYNAME"))
                        .unwrap_or_default();
                    
                    let size_bytes = Self::extract_xml_tag(image_block, "TOTALBYTES")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0);
                    
                    let installation_type = Self::extract_xml_tag(image_block, "INSTALLATIONTYPE")
                        .unwrap_or_default();

                    println!(
                        "[DISM XML] 索引: {}, 名称: {}, 安装类型: {}, 大小: {}",
                        index, name, installation_type, size_bytes
                    );

                    if index > 0 && !name.is_empty() {
                        images.push(ImageInfo {
                            index,
                            name,
                            size_bytes,
                            installation_type,
                        });
                    }

                    pos = abs_start + image_end + 8;
                } else {
                    pos = abs_start + 14;
                }
            } else {
                pos = abs_start + 14;
            }
        }

        if images.is_empty() {
            anyhow::bail!("未找到有效的镜像信息");
        }

        Ok(images)
    }

    /// 从 XML 块中提取指定标签的内容
    fn extract_xml_tag(xml: &str, tag: &str) -> Option<String> {
        let open_tag = format!("<{}>", tag);
        let close_tag = format!("</{}>", tag);
        
        if let Some(start) = xml.find(&open_tag) {
            let content_start = start + open_tag.len();
            if let Some(end) = xml[content_start..].find(&close_tag) {
                let content = &xml[content_start..content_start + end];
                return Some(content.trim().to_string());
            }
        }
        None
    }

    /// 解析 DISM 基本输出（不包含 Installation Type）
    fn parse_basic_image_info(output: &str) -> Result<Vec<ImageInfo>> {
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
                        installation_type: String::new(), // 稍后填充
                    });
                }
                if let Some(num) = line.split(':').nth(1) {
                    current_index = num.trim().parse().unwrap_or(0);
                }
                current_name.clear();
                current_size = 0;
            } else if line.contains("名称") || line.contains("Name") {
                // 避免匹配到 "产品名称" 等
                let is_name_line = line.starts_with("名称") 
                    || line.starts_with("Name")
                    || line.contains(": ") && (
                        line.split(':').next().map(|s| s.trim()) == Some("名称")
                        || line.split(':').next().map(|s| s.trim()) == Some("Name")
                    );
                if is_name_line {
                    if let Some(name) = line.split(':').nth(1) {
                        current_name = name.trim().to_string();
                    }
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
                installation_type: String::new(),
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
        use std::io::Read;
        
        println!("[DISM] 启动命令: {} {:?}", &self.dism_path, args);
        
        let mut child = create_command(&self.dism_path)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        if let Some(mut stdout) = child.stdout.take() {
            let mut buffer = [0u8; 1024];
            let mut line_buffer = Vec::new();
            
            loop {
                match stdout.read(&mut buffer) {
                    Ok(0) => break, // EOF
                    Ok(n) => {
                        // 打印原始字节用于调试
                        println!("[DISM RAW] 读取 {} 字节", n);
                        
                        for &byte in &buffer[..n] {
                            // \r 或 \n 都作为行结束符
                            if byte == b'\r' || byte == b'\n' {
                                if !line_buffer.is_empty() {
                                    let line = gbk_to_utf8(&line_buffer);
                                    println!("[DISM LINE] {}", line);
                                    
                                    if let Some(progress) = Self::parse_progress_line(&line) {
                                        println!("[DISM PROGRESS] 解析到进度: {}%", progress.percentage);
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
                        println!("[DISM ERROR] 读取错误: {:?}", e);
                        if e.kind() != std::io::ErrorKind::Interrupted {
                            break;
                        }
                    }
                }
            }
            
            // 处理剩余的数据
            if !line_buffer.is_empty() {
                let line = gbk_to_utf8(&line_buffer);
                println!("[DISM FINAL] {}", line);
                if let Some(progress) = Self::parse_progress_line(&line) {
                    println!("[DISM PROGRESS] 最终进度: {}%", progress.percentage);
                    if let Some(ref tx) = progress_tx {
                        let _ = tx.send(progress);
                    }
                }
            }
        }

        // 同时读取 stderr
        if let Some(mut stderr) = child.stderr.take() {
            let mut err_output = Vec::new();
            let _ = stderr.read_to_end(&mut err_output);
            if !err_output.is_empty() {
                let err_str = gbk_to_utf8(&err_output);
                println!("[DISM STDERR] {}", err_str);
            }
        }

        let status = child.wait()?;
        println!("[DISM] 命令结束，状态: {:?}", status);
        if !status.success() {
            anyhow::bail!("DISM command failed");
        }
        Ok(())
    }

    /// 解析 DISM 进度输出
    /// DISM 输出格式示例:
    /// [==                    ]  10.0%
    /// [=====                 ] 25%
    /// 正在捕获映像 [===========         ]  55.0%
    fn parse_progress_line(line: &str) -> Option<DismProgress> {
        // 方法1: 直接查找百分比数字
        // 匹配格式: 数字 + 可选小数 + %
        let mut percentage: Option<u8> = None;

        // 查找 xx.x% 或 xx% 格式
        let line_chars: Vec<char> = line.chars().collect();
        for i in 0..line_chars.len() {
            if line_chars[i] == '%' && i > 0 {
                // 向前查找数字
                let mut num_str = String::new();
                let mut j = i as i32 - 1;
                
                // 跳过可能的小数部分
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
                    // 解析数字，取整数部分
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

        // 方法2: 如果上面没找到，尝试旧的解析方式
        if percentage.is_none() && line.contains('[') && line.contains(']') {
            if let Some(after_bracket) = line.split(']').nth(1) {
                let cleaned = after_bracket
                    .trim()
                    .replace("=", "")
                    .replace(" ", "")
                    .replace(".0", "")
                    .replace("%", "");
                if let Ok(p) = cleaned.parse::<u8>() {
                    percentage = Some(p);
                }
            }
        }

        percentage.map(|p| DismProgress {
            percentage: p,
            status: format!("{}%", p),
        })
    }

    /// 获取系统信息 (离线)
    pub fn get_offline_system_info(&self, image_path: &str) -> Result<String> {
        let output = create_command(&self.dism_path)
            .args([
                &format!("/image:{}", image_path),
                "/get-currentedition",
            ])
            .output()?;

        Ok(gbk_to_utf8(&output.stdout))
    }

    /// 清理组件存储
    pub fn cleanup_image(&self, image_path: &str) -> Result<()> {
        let args = if self.is_pe {
            vec![
                format!("/image:{}", image_path),
                "/cleanup-image".to_string(),
                "/startcomponentcleanup".to_string(),
            ]
        } else {
            vec![
                "/online".to_string(),
                "/cleanup-image".to_string(),
                "/startcomponentcleanup".to_string(),
            ]
        };

        let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        
        let output = create_command(&self.dism_path)
            .args(&args_ref)
            .output()?;

        if !output.status.success() {
            let stderr = gbk_to_utf8(&output.stderr);
            anyhow::bail!("Failed to cleanup image: {}", stderr);
        }
        Ok(())
    }
}

impl Default for Dism {
    fn default() -> Self {
        Self::new()
    }
}
