//! XP / 2003 文本安装阶段的存储驱动集成（nLite / WinNTSetup 风格）。
//!
//! 让原版 XP/2003 文本安装（蓝底）阶段就能认 AHCI / NVMe 等控制器：把驱动的 `.sys`
//! 拷进本地源，并在 `txtsetup.sif` 写 `[SourceDisksFiles]` / `[SCSI.Load]` / `[SCSI]` /
//! `[HardwareIdsDatabase]`。文本安装引擎据此在文本阶段加载 miniport 认盘，并把服务登记进
//! 目标系统（等价于 WIM 路 `xp::inject_xp_drivers` 的「登记服务 + CriticalDeviceDatabase」）。
//!
//! 按架构扫描驱动目录（i386 源用 x86 驱动、amd64 源用 amd64 驱动），逐个解析其 `.inf`。
//! 解析失败（缺服务名 / 缺硬件 ID / 无 .sys）的目录会被跳过，不会污染 txtsetup.sif。

use std::path::{Path, PathBuf};

/// 解析出的一个文本期存储驱动。
#[derive(Debug, Clone)]
pub struct TxtmodeDriver {
    /// miniport 服务名，如 `genahci`。
    pub service: String,
    /// miniport 的 `.sys` 文件名，如 `genahci.sys`。
    pub miniport_sys: String,
    /// 友好描述（写入 `[SCSI]`）。
    pub desc: String,
    /// 硬件 ID，如 `PCI\CC_010601`（写入 `[HardwareIdsDatabase]`）。
    pub hwids: Vec<String>,
    /// 该驱动目录里所有 `.sys`（含 storport/ntoskrn8 等依赖），全部拷进源。
    pub sys_files: Vec<PathBuf>,
}

/// `.inf` 解析出的核心信息（与文件系统无关，便于单测）。
struct ParsedInf {
    service: String,
    miniport_sys: String,
    hwids: Vec<String>,
}

/// 在若干根目录下递归扫描驱动（每个含可解析 `.inf` 的目录视为一个驱动）。
pub fn scan_driver_roots(roots: &[PathBuf]) -> Vec<TxtmodeDriver> {
    let mut found = Vec::new();
    for root in roots {
        if root.is_dir() {
            collect_infs(root, &mut found, 0);
        }
    }
    found
}

fn collect_infs(dir: &Path, out: &mut Vec<TxtmodeDriver>, depth: usize) {
    if depth > 4 {
        return;
    }
    let rd = match std::fs::read_dir(dir) {
        Ok(r) => r,
        Err(_) => return,
    };
    let mut subdirs = Vec::new();
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            subdirs.push(p);
        } else if p
            .extension()
            .map(|x| x.eq_ignore_ascii_case("inf"))
            .unwrap_or(false)
        {
            if let Some(d) = parse_driver_inf(&p) {
                out.push(d);
            }
        }
    }
    for s in subdirs {
        collect_infs(&s, out, depth + 1);
    }
}

/// 解析单个 `.inf` + 收集同目录下的 `.sys`。信息不全返回 `None`。
pub fn parse_driver_inf(inf: &Path) -> Option<TxtmodeDriver> {
    let raw = std::fs::read(inf).ok()?;
    let text = decode_inf(&raw);
    let parsed = parse_inf_text(&text)?;
    let dir = inf.parent()?;
    let mut sys_files = Vec::new();
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        let p = e.path();
        if p.extension()
            .map(|x| x.eq_ignore_ascii_case("sys"))
            .unwrap_or(false)
        {
            sys_files.push(p);
        }
    }
    if sys_files.is_empty() {
        return None;
    }
    sys_files.sort();
    Some(TxtmodeDriver {
        desc: format!("{} (LetRecovery textmode)", parsed.service),
        service: parsed.service,
        miniport_sys: parsed.miniport_sys,
        hwids: parsed.hwids,
        sys_files,
    })
}

/// 把驱动拷进源（`source_sub`，如 `C:\$WIN_NT$.~LS\I386`）并合并进 `txtsetup`。
/// 返回（合并后的 txtsetup.sif 内容, 日志）。
pub fn integrate(txtsetup: &str, drivers: &[TxtmodeDriver], source_sub: &Path) -> (String, String) {
    let mut log = String::new();
    if drivers.is_empty() {
        log.push_str("[TXTDRV] 未发现可集成的文本期存储驱动（跳过）\n");
        return (txtsetup.to_string(), log);
    }

    let mut source_disks_files: Vec<String> = Vec::new();
    let mut scsi_load: Vec<String> = Vec::new();
    let mut scsi: Vec<String> = Vec::new();
    let mut hwid_db: Vec<String> = Vec::new();

    for d in drivers {
        // 1) 拷所有 .sys 进源（覆盖同名——让魔改 storport.sys 替换原版）。
        for sys in &d.sys_files {
            let name = match sys.file_name().and_then(|s| s.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };
            let dst = source_sub.join(&name);
            match std::fs::copy(sys, &dst) {
                Ok(_) => log.push_str(&format!("[TXTDRV] 拷入源: {}\n", name)),
                Err(e) => {
                    log.push_str(&format!("[TXTDRV] 警告: 拷入 {} 失败: {}\n", name, e));
                    continue;
                }
            }
            // [SourceDisksFiles] 仅在原 txtsetup 没有该键时加（避免与原版 storport.sys 等重复键）。
            if !section_has_key(txtsetup, "[SourceDisksFiles]", &name) {
                source_disks_files.push(format!("{} = 1,,,,,,4_,4,1,,,1,4", name));
            }
        }
        // 2) miniport 的文本期加载/描述/硬件 ID 绑定。
        scsi_load.push(format!("{} = {},4", d.service, d.miniport_sys));
        scsi.push(format!("{} = \"{}\"", d.service, d.desc));
        for h in &d.hwids {
            hwid_db.push(format!("{} = \"{}\"", h, d.service));
        }
        log.push_str(&format!(
            "[TXTDRV] 集成 {}（miniport {}，{} 个硬件 ID）\n",
            d.service,
            d.miniport_sys,
            d.hwids.len()
        ));
    }

    let mut out = txtsetup.to_string();
    out = append_to_section(&out, "[SourceDisksFiles]", &source_disks_files);
    out = append_to_section(&out, "[SCSI.Load]", &scsi_load);
    out = append_to_section(&out, "[SCSI]", &scsi);
    out = append_to_section(&out, "[HardwareIdsDatabase]", &hwid_db);
    (out, log)
}

// ───────────────────────── 内部解析/合并 ─────────────────────────

fn decode_inf(raw: &[u8]) -> String {
    if raw.len() >= 2 && raw[0] == 0xFF && raw[1] == 0xFE {
        let u16s: Vec<u16> = raw[2..]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&u16s)
    } else {
        match std::str::from_utf8(raw) {
            Ok(s) => s.to_string(),
            Err(_) => crate::encoding::gbk_to_utf8(raw),
        }
    }
}

/// 解析 INF 文本为有序的 (节名, 行集合)。去注释(`;`)、去空行。
fn parse_sections(text: &str) -> Vec<(String, Vec<String>)> {
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    let mut cur: Option<usize> = None;
    for raw in text.lines() {
        let line = match raw.find(';') {
            Some(i) => &raw[..i],
            None => raw,
        };
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if t.starts_with('[') && t.ends_with(']') {
            let name = t[1..t.len() - 1].trim().to_string();
            out.push((name, Vec::new()));
            cur = Some(out.len() - 1);
        } else if let Some(i) = cur {
            out[i].1.push(t.to_string());
        }
    }
    out
}

fn parse_inf_text(text: &str) -> Option<ParsedInf> {
    let sections = parse_sections(text);
    let get = |name: &str| -> Vec<&str> {
        sections
            .iter()
            .filter(|(n, _)| n.eq_ignore_ascii_case(name))
            .flat_map(|(_, ls)| ls.iter().map(|s| s.as_str()))
            .collect()
    };

    // 1) 服务名 + 服务安装节：扫所有以 ".Services" 结尾的节里的 AddService。
    let mut service: Option<String> = None;
    let mut svc_inst: Option<String> = None;
    for (name, lines) in &sections {
        if name.to_ascii_lowercase().ends_with(".services") {
            for l in lines {
                if let Some(rhs) = strip_key_ci(l, "addservice") {
                    let fields: Vec<&str> = rhs.split(',').map(|s| s.trim()).collect();
                    if !fields.is_empty() && !fields[0].is_empty() {
                        service = Some(fields[0].to_string());
                        if fields.len() >= 3 && !fields[2].is_empty() {
                            svc_inst = Some(fields[2].to_string());
                        }
                        break;
                    }
                }
            }
        }
        if service.is_some() {
            break;
        }
    }
    let service = service?;

    // 2) miniport .sys：服务安装节里的 ServiceBinary = %12%\xxx.sys；缺则回退 <service>.sys。
    let mut miniport_sys = format!("{}.sys", service);
    if let Some(si) = &svc_inst {
        for l in get(si) {
            if let Some(rhs) = strip_key_ci(l, "servicebinary") {
                if let Some(name) = rhs.rsplit(['\\', '/']).next() {
                    let name = name.trim();
                    if name.to_ascii_lowercase().ends_with(".sys") {
                        miniport_sys = name.to_string();
                    }
                }
            }
        }
    }

    // 3) 硬件 ID：据 [Manufacturer] 找型号节，从型号行 RHS 提取 PCI\ 标识。
    let mut hwids: Vec<String> = Vec::new();
    for l in get("Manufacturer") {
        if let Some((_, rhs)) = l.split_once('=') {
            let parts: Vec<&str> = rhs.split(',').map(|s| s.trim()).collect();
            if parts.is_empty() || parts[0].is_empty() {
                continue;
            }
            let base = parts[0];
            let mut model_secs = vec![base.to_string()];
            for ext in &parts[1..] {
                if !ext.is_empty() {
                    model_secs.push(format!("{}.{}", base, ext));
                }
            }
            for ms in &model_secs {
                for ml in get(ms) {
                    if let Some((_, mrhs)) = ml.split_once('=') {
                        for h in extract_hwids(mrhs) {
                            if !hwids.iter().any(|x| x.eq_ignore_ascii_case(&h)) {
                                hwids.push(h);
                            }
                        }
                    }
                }
            }
        }
    }
    if hwids.is_empty() {
        return None;
    }

    Some(ParsedInf {
        service,
        miniport_sys,
        hwids,
    })
}

/// 行形如 `Key = Value` 且 Key（去空白、忽略大小写）等于 `key` 时，返回去空白的 Value。
fn strip_key_ci<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let (k, v) = line.split_once('=')?;
    if k.trim().eq_ignore_ascii_case(key) {
        Some(v.trim())
    } else {
        None
    }
}

/// 从一段文本里提取所有 `PCI\....` 形式的硬件 ID（大小写不敏感地定位前缀）。
fn extract_hwids(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = s.as_bytes();
    let lower = s.to_ascii_lowercase();
    let mut i = 0usize;
    while let Some(rel) = lower[i..].find("pci\\") {
        let start = i + rel;
        let mut end = start + 4;
        while end < bytes.len() {
            let c = bytes[end] as char;
            if c.is_ascii_alphanumeric() || c == '_' || c == '&' || c == '.' {
                end += 1;
            } else {
                break;
            }
        }
        out.push(s[start..end].to_string());
        i = end.max(start + 4);
    }
    out
}

/// 某 INI 节内是否已有键 `key`（大小写不敏感）。
fn section_has_key(content: &str, section: &str, key: &str) -> bool {
    let mut in_sec = false;
    for line in content.lines() {
        let t = line.trim();
        if t.starts_with('[') && t.ends_with(']') {
            in_sec = t.eq_ignore_ascii_case(section);
            continue;
        }
        if in_sec {
            if let Some((k, _)) = t.split_once('=') {
                if k.trim().eq_ignore_ascii_case(key) {
                    return true;
                }
            }
        }
    }
    false
}

/// 在 `[section]` 节标题行后追加 `lines`（CRLF）。节不存在则在文末新建。
fn append_to_section(content: &str, section: &str, lines: &[String]) -> String {
    if lines.is_empty() {
        return content.to_string();
    }
    let mut out = String::with_capacity(content.len() + 256);
    let mut inserted = false;
    for line in content.split_inclusive('\n') {
        out.push_str(line);
        if !inserted && line.trim().eq_ignore_ascii_case(section) {
            for l in lines {
                out.push_str(l);
                out.push_str("\r\n");
            }
            inserted = true;
        }
    }
    if !inserted {
        if !out.is_empty() && !out.ends_with('\n') {
            out.push_str("\r\n");
        }
        out.push_str(section);
        out.push_str("\r\n");
        for l in lines {
            out.push_str(l);
            out.push_str("\r\n");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const GENAHCI_INF: &str = "\
[Version]
Signature=\"$Windows NT$\"
[Manufacturer]
%MANUFACTURER% = Models, NTamd64.5.2
[Models.NTamd64.5.2]
%ADAPTERNAME%=genahci, PCI\\CC_010601
[genahci.Services]
AddService = genahci, 2, Service
[Service]
StartType      = 0
ServiceBinary  = %12%\\genahci.sys
";

    const STORNVME_INF: &str = "\
[Manufacturer]
%MS-NVME% = NVME, NTamd64.5.2
[NVME.NTamd64.5.2]
%PCI\\CC_010802.DeviceDesc%=Stornvme_Inst, PCI\\CC_010802
[Stornvme_Inst.Services]
AddService = stornvme, 0x00000002, Stornvme_Service_Inst, Stornvme_EventLog_Inst
[Stornvme_Service_Inst]
ServiceBinary  = %12%\\stornvme.sys
";

    #[test]
    fn parse_genahci() {
        let p = parse_inf_text(GENAHCI_INF).expect("should parse");
        assert_eq!(p.service, "genahci");
        assert_eq!(p.miniport_sys, "genahci.sys");
        assert_eq!(p.hwids, vec!["PCI\\CC_010601"]);
    }

    #[test]
    fn parse_stornvme_ignores_lhs_pci_token() {
        let p = parse_inf_text(STORNVME_INF).expect("should parse");
        assert_eq!(p.service, "stornvme");
        assert_eq!(p.miniport_sys, "stornvme.sys");
        // 只取 RHS 的硬件 ID，左侧 %PCI\CC_010802.DeviceDesc% 不算
        assert_eq!(p.hwids, vec!["PCI\\CC_010802"]);
    }

    #[test]
    fn parse_rejects_without_hwid() {
        let inf = "[Foo.Services]\nAddService = foo, 2, S\n[S]\nServiceBinary=%12%\\foo.sys\n";
        assert!(parse_inf_text(inf).is_none());
    }

    #[test]
    fn extract_hwids_from_rhs() {
        assert_eq!(extract_hwids("genahci, PCI\\CC_010601"), vec!["PCI\\CC_010601"]);
        assert_eq!(
            extract_hwids("x, PCI\\VEN_8086&DEV_2829&CC_0106"),
            vec!["PCI\\VEN_8086&DEV_2829&CC_0106"]
        );
        assert!(extract_hwids("no ids here").is_empty());
    }

    #[test]
    fn append_inserts_after_existing_header() {
        let ts = "[SCSI]\r\natapi = \"IDE\"\r\n\r\n[SCSI.Load]\r\natapi = atapi.sys,4\r\n";
        let out = append_to_section(ts, "[SCSI.Load]", &["genahci = genahci.sys,4".to_string()]);
        assert!(out.contains("[SCSI.Load]\r\ngenahci = genahci.sys,4\r\natapi = atapi.sys,4"));
    }

    #[test]
    fn append_creates_missing_section() {
        let out = append_to_section("[Foo]\r\nx=1\r\n", "[HardwareIdsDatabase]", &["a = \"b\"".to_string()]);
        assert!(out.contains("[HardwareIdsDatabase]\r\na = \"b\"\r\n"));
    }

    #[test]
    fn section_has_key_detects_dup() {
        let ts = "[SourceDisksFiles]\r\nstorport.sys = 1,,,\r\n";
        assert!(section_has_key(ts, "[SourceDisksFiles]", "storport.sys"));
        assert!(section_has_key(ts, "[SourceDisksFiles]", "STORPORT.SYS"));
        assert!(!section_has_key(ts, "[SourceDisksFiles]", "genahci.sys"));
    }

    #[test]
    fn integrate_merges_all_sections() {
        let ts = "[SourceDisksFiles]\r\nstorport.sys = 1,,x\r\n[SCSI.Load]\r\n[SCSI]\r\n[HardwareIdsDatabase]\r\n";
        let drv = TxtmodeDriver {
            service: "genahci".into(),
            miniport_sys: "genahci.sys".into(),
            desc: "genahci (LetRecovery textmode)".into(),
            hwids: vec!["PCI\\CC_010601".into()],
            sys_files: vec![], // 无文件可拷（测纯合并；拷贝在真实路径做）
        };
        let (out, _log) = integrate(ts, &[drv], Path::new("/nonexistent-source"));
        assert!(out.contains("[SCSI.Load]\r\ngenahci = genahci.sys,4"));
        assert!(out.contains("genahci = \"genahci (LetRecovery textmode)\""));
        assert!(out.contains("PCI\\CC_010601 = \"genahci\""));
        // storport.sys 已存在不重复加；这里没有可拷 .sys 故 SourceDisksFiles 不新增
    }
}
