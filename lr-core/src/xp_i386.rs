//! Windows XP / 2003 的「i386 硬盘文本模式安装」（仅 Legacy/MBR）。
//!
//! 照搬成熟工具 DSI-安装备份（PECMD `dsi.WCS`）的「WinPE 无光驱硬盘装 NT5」做法——即微软
//! `winnt32 /makelocalsource` 原生的 `$WIN_NT$.~LS` + `$WIN_NT$.~BT` 约定。原版 XP/2003 安装盘是
//! `\I386`（或 x64 的 `\AMD64`）文本安装结构，没有 Vista+ 的 `install.wim`，无法「释放 WIM」。
//! 流程：
//!
//!   1. 把 `<arch>`（I386/AMD64）整个复制到 `\$WIN_NT$.~LS\<arch>`（本地源）；建空 `$WIN_NT$.~LS\$OEM$`；
//!   2. 建 `$WIN_NT$.~BT`（文本启动阶段的 BootPath）：拷 `<arch>\SYSTEM32` 整目录 + 按内嵌
//!      `NT5.txt` 清单把 `<arch>\<名>` 原样（压缩名不解压）复制进去；
//!   3. 根目录：`setupldr.bin`→`NTLDR`（开机直接进文本安装）、`NTDETECT.COM`、`BOOTFONT.BIN`、
//!      `TXTSETUP.SIF`（含文本期存储驱动集成）；`TXTSETUP.SIF` 同样写一份进 `$WIN_NT$.~BT`；
//!   4. `WINNT.SIF` 写进 `$WIN_NT$.~BT\`，并【强制】`MsDosInitiated=1`/`Floppyless=1`/`AutoPartition=0`/
//!      `UnattendedInstall=Yes`/`OemPreinstall=Yes`（缺 `MsDosInitiated=1` 文本安装会去找光盘而失败）；
//!      不改 `txtsetup.sif` 的 `SetupSourcePath`（靠 `MsDosInitiated=1` + `$WIN_NT$.~BT` 约定）；
//!   5. 标记目标分区为「活动分区」+ `bootsect /nt52` 写 XP 引导码（MBR/引导扇区加载 NTLDR）。
//!
//! 重启后 → setupldr 据 `$WIN_NT$.~BT` 进入 XP/2003 文本安装（蓝底）→ 复制文件 → 再次重启 → 图形安装。
//!
//! 限制：**仅 Legacy/BIOS + MBR**。XP 不支持 GPT/UEFI（调用方在 UI 已拦截 GPT/UEFI 目标）。
//! 调用前目标盘需已格式化(NTFS/FAT32)、且应为目标磁盘上的主分区。

use std::path::Path;
use std::thread::sleep;
use std::time::Duration;

use crate::command::new_command;
use crate::encoding::gbk_to_utf8;

/// `$WIN_NT$.~BT` 引导文件清单（编译期嵌入，照搬 DSI nt5\NT5.txt）。
const NT5_BOOTFILES: &str = include_str!("xp_nt5_bootfiles.txt");

/// 遍历 `$WIN_NT$.~BT` 引导文件清单（去注释 `#`、去空行、去首尾空白）。
fn nt5_bootfiles() -> impl Iterator<Item = &'static str> {
    NT5_BOOTFILES
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
}

/// 判断某目录是否为有效的 XP/2003 i386 安装源（含 setupldr.bin + txtsetup.sif + ntdetect.com）。
pub fn is_valid_i386(dir: &Path) -> bool {
    dir.join("setupldr.bin").exists()
        && dir.join("txtsetup.sif").exists()
        && dir.join("ntdetect.com").exists()
}

/// 从 i386 源目录做硬盘文本安装准备。成功后重启即进入 XP 文本安装。
///
/// - `i386_src`：i386 目录（如挂载 ISO 的 `G:\I386`，或已复制到数据分区的副本）。
/// - `win_partition`：目标系统盘（如 `"C:"`），需已格式化且为目标磁盘的主分区。
/// - `bin_dir`：程序 bin 目录（取 `bootsect.exe`；可选 `bin\xp\productkey.txt` 提供产品密钥实现全自动）。
/// - `custom_sif`：用户自定义的 winnt.sif 应答文件路径；`Some` 且存在时直接用它（原样写入，
///   规整为 CRLF），否则用内置生成的应答（按是否有产品密钥决定 DefaultHide/FullUnattended）。
pub fn install_from_i386(
    i386_src: &Path,
    win_partition: &str,
    bin_dir: &Path,
    custom_sif: Option<&Path>,
) -> Result<String, String> {
    let win = win_partition.trim_end_matches('\\'); // "C:"
    let mut log = String::new();

    // 0) 源目录 + 源中三件套校验
    if !i386_src.exists() {
        return Err(format!("找不到 i386 源目录: {}", i386_src.display()));
    }
    let setupldr = i386_src.join("setupldr.bin");
    let txtsetup = i386_src.join("txtsetup.sif");
    let ntdetect = i386_src.join("ntdetect.com");
    for (p, n) in [
        (&setupldr, "setupldr.bin"),
        (&txtsetup, "txtsetup.sif"),
        (&ntdetect, "ntdetect.com"),
    ] {
        if !p.exists() {
            return Err(format!("i386 缺少 {}，不是有效的 XP/2003 安装源", n));
        }
    }

    // 0.5) 关键修复：确认目标分区根目录此刻【真的可写】，带重试。
    //      之前实机报「创建 C:\$WIN_NT$.~LS\I386 失败: 系统找不到指定的路径 (os error 3)」即出在
    //      下一步的 create_dir：刚格式化结束时盘符可能短暂卸载/重挂，或所选盘符当前并未挂载。
    //      这里先带重试探测一遍，过不了就给出可读的原因，而不是让 create_dir 抛裸 os error 3。
    ensure_volume_ready(win).map_err(|e| {
        format!(
            "目标分区 {win} 当前不可写：{e}。请确认该分区已分配盘符、且已格式化为 NTFS/FAT32（若刚格式化完，请稍候重试）。XP 仅支持 Legacy/MBR，目标盘不能是 GPT。"
        )
    })?;
    log.push_str(&format!("目标分区 {win} 可写，开始准备文本安装\n"));

    // 1) 复制 源(I386/AMD64) → win\$WIN_NT$.~LS\<同名子目录>（文本安装本地源）。
    //    子目录名取源目录名(I386 或 AMD64)，与 txtsetup.sif 的 [SourceDisksNames] 路径一致；
    //    对原版 32 位 i386 介质即 I386(行为不变)，64 位 2003/XP x64 介质则为 AMD64。
    let src_sub_name = i386_src
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("I386")
        .to_uppercase();
    let ls_src = format!("{win}\\$WIN_NT$.~LS\\{src_sub_name}");
    log.push_str(&format!("复制 {src_sub_name} 源到 {ls_src} ...\n"));
    create_dir_all_retry(&ls_src).map_err(|e| format!("创建 {ls_src} 失败: {e}"))?;
    let src = i386_src.to_string_lossy().to_string();
    let out = new_command("xcopy")
        .args([src.as_str(), ls_src.as_str(), "/E", "/I", "/H", "/R", "/Y", "/Q"])
        .output()
        .map_err(|e| format!("xcopy 执行失败: {e}"))?;
    log.push_str(&gbk_to_utf8(&out.stdout));
    if !out.status.success() {
        return Err(format!("复制 i386 失败:\n{}", gbk_to_utf8(&out.stderr)));
    }

    // 1.5) 建空 $OEM$（OemPreinstall=Yes 需要它存在；空目录无副作用）。
    let _ = create_dir_all_retry(&format!("{win}\\$WIN_NT$.~LS\\$OEM$"));

    // 1.6) 建 $WIN_NT$.~BT（文本启动阶段的 BootPath）：照搬 DSI——
    //      a) 整个 <arch>\SYSTEM32 → $WIN_NT$.~BT\SYSTEM32；
    //      b) 按 NT5.txt 清单把 <arch>\<名> 原样（压缩名不解压）→ $WIN_NT$.~BT\<名>。
    //      setupldr(伪装 NTLDR) 据 $WIN_NT$.~BT 的存在进入「硬盘本地源安装」，从这里加载文本期内核/驱动。
    let bt = format!("{win}\\$WIN_NT$.~BT");
    create_dir_all_retry(&bt).map_err(|e| format!("创建 {bt} 失败: {e}"))?;
    let sys32_src = i386_src.join("SYSTEM32");
    if sys32_src.exists() {
        let s = sys32_src.to_string_lossy().to_string();
        let d = format!("{bt}\\SYSTEM32");
        let o = new_command("xcopy")
            .args([s.as_str(), d.as_str(), "/E", "/I", "/H", "/R", "/Y", "/Q"])
            .output()
            .map_err(|e| format!("拷 SYSTEM32 → $WIN_NT$.~BT 失败: {e}"))?;
        if o.status.success() {
            log.push_str("已复制 <源>\\SYSTEM32 → $WIN_NT$.~BT\\SYSTEM32\n");
        } else {
            log.push_str(&format!(
                "警告: 拷 SYSTEM32 → $WIN_NT$.~BT 非 0：{}\n",
                gbk_to_utf8(&o.stderr)
            ));
        }
    } else {
        log.push_str("警告: 源中无 SYSTEM32 子目录（部分重封装介质如此），$WIN_NT$.~BT\\SYSTEM32 跳过\n");
    }
    let (mut bt_copied, mut bt_missing) = (0usize, 0usize);
    for name in nt5_bootfiles() {
        let s = i386_src.join(name);
        if s.exists() {
            match std::fs::copy(&s, format!("{bt}\\{name}")) {
                Ok(_) => bt_copied += 1,
                Err(e) => log.push_str(&format!("警告: 拷 {name} → $WIN_NT$.~BT 失败: {e}\n")),
            }
        } else {
            bt_missing += 1;
        }
    }
    log.push_str(&format!(
        "$WIN_NT$.~BT 引导文件：按清单复制 {bt_copied} 个（源中缺 {bt_missing} 个，已跳过）\n"
    ));

    // 2) 引导文件落根目录：
    //    setupldr.bin -> \NTLDR（开机直接进文本安装）；ntdetect.com -> \NTDETECT.COM；
    //    biosinfo.inf / bootfont.bin 若源里有也一并复制（setupldr 需要 biosinfo；bootfont 为蓝底本地化字库）。
    std::fs::copy(&setupldr, format!("{win}\\NTLDR")).map_err(|e| format!("写 NTLDR 失败: {e}"))?;
    std::fs::copy(&ntdetect, format!("{win}\\NTDETECT.COM"))
        .map_err(|e| format!("写 NTDETECT.COM 失败: {e}"))?;
    log.push_str("已写入 NTLDR(setupldr) / NTDETECT.COM\n");
    for opt in ["biosinfo.inf", "bootfont.bin"] {
        let s = i386_src.join(opt);
        if s.exists() {
            match std::fs::copy(&s, format!("{win}\\{opt}")) {
                Ok(_) => log.push_str(&format!("已复制 {opt}\n")),
                Err(e) => log.push_str(&format!("复制 {opt} 失败（忽略）: {e}\n")),
            }
        }
    }

    // 3) txtsetup.sif：照搬 DSI——不改 SetupSourcePath（靠 MsDosInitiated=1 + $WIN_NT$.~BT 约定，
    //    setupldr/setupdd 自会用 $WIN_NT$.~BT 作引导路径、$WIN_NT$.~LS 作源）。仅做文本期驱动集成，
    //    再写入 $WIN_NT$.~BT\TXTSETUP.SIF（setupldr 实际读这份）与根目录各一份。
    let raw = std::fs::read(&txtsetup).map_err(|e| format!("读 txtsetup.sif 失败: {e}"))?;
    let txt = match String::from_utf8(raw.clone()) {
        Ok(s) => s,
        Err(_) => gbk_to_utf8(&raw),
    };
    let txt = normalize_crlf(&txt);

    // 文本期存储驱动集成（按架构）：驱动 .sys 同时拷进源($WIN_NT$.~LS\<arch>)与引导($WIN_NT$.~BT)。
    let xp_drv = bin_dir.join("drivers").join("xp");
    let roots = if src_sub_name == "AMD64" {
        vec![xp_drv.join("amd64"), xp_drv.join("ahci"), xp_drv.join("nvme")]
    } else {
        vec![xp_drv.join("x86")]
    };
    let drivers = crate::xp_textmode_drv::scan_driver_roots(&roots);
    log.push_str(&format!(
        "文本期存储驱动：架构={}，发现 {} 个可集成驱动\n",
        if src_sub_name == "AMD64" { "amd64" } else { "x86" },
        drivers.len()
    ));
    let (final_txtsetup, drvlog) =
        crate::xp_textmode_drv::integrate(&txt, &drivers, &[Path::new(&ls_src), Path::new(&bt)]);
    log.push_str(&drvlog);

    std::fs::write(format!("{bt}\\TXTSETUP.SIF"), final_txtsetup.as_bytes())
        .map_err(|e| format!("写 $WIN_NT$.~BT\\TXTSETUP.SIF 失败: {e}"))?;
    std::fs::write(format!("{win}\\TXTSETUP.SIF"), final_txtsetup.as_bytes())
        .map_err(|e| format!("写根 TXTSETUP.SIF 失败: {e}"))?;
    log.push_str("已写入 TXTSETUP.SIF（$WIN_NT$.~BT 与根；含文本期驱动集成）\n");

    // 4) winnt.sif 应答：优先用户自定义；否则内置生成。无论哪种，都【强制写入硬盘安装必需的键】
    //    （照搬 DSI 的 NT5部署无人值守：MsDosInitiated=1 / Floppyless=1 / AutoPartition=0 /
    //    UnattendedInstall=Yes / OemPreinstall=Yes）——缺它们文本安装会去找光盘而失败。
    //    放在 $WIN_NT$.~BT\WINNT.SIF（文本安装阶段读这份）。
    let sif_raw = match custom_sif {
        Some(p) if p.exists() => {
            let raw = std::fs::read(p)
                .map_err(|e| format!("读自定义 winnt.sif 失败 {}: {e}", p.display()))?;
            let s = match String::from_utf8(raw.clone()) {
                Ok(s) => s,
                Err(_) => gbk_to_utf8(&raw),
            };
            log.push_str(&format!("使用自定义无人值守应答: {}\n", p.display()));
            normalize_crlf(&s)
        }
        _ => {
            let product_key = read_product_key(bin_dir);
            match &product_key {
                Some(_) => {
                    log.push_str("检测到产品密钥（bin\\xp\\productkey.txt）→ 全自动无人值守\n")
                }
                None => log.push_str(
                    "未提供产品密钥（可放 bin\\xp\\productkey.txt 实现全自动）→ 仅「密钥」页停顿，其余无人值守\n",
                ),
            }
            winnt_sif(product_key.as_deref())
        }
    };
    let sif_content = force_winnt_keys(&sif_raw);
    std::fs::write(format!("{bt}\\WINNT.SIF"), sif_content.as_bytes())
        .map_err(|e| format!("写 $WIN_NT$.~BT\\WINNT.SIF 失败: {e}"))?;
    log.push_str("已写入 $WIN_NT$.~BT\\WINNT.SIF（已强制 MsDosInitiated=1 等硬盘安装必需键）\n");

    // 4.5) 标记目标分区为「活动分区」。Legacy/MBR BIOS 必须从活动分区加载 NTLDR；
    //      本引擎只支持 Legacy/MBR（GPT/UEFI 目标由调用方拦截），故此处直接置活动。失败仅告警。
    let letter = win.trim_end_matches(':');
    match set_volume_active(letter) {
        Ok(o) => {
            log.push_str(&format!("已标记 {win} 为活动分区\n"));
            let o = o.trim();
            if !o.is_empty() {
                log.push_str(o);
                log.push('\n');
            }
        }
        Err(e) => log.push_str(&format!(
            "警告: 标记活动分区失败（{e}）。若目标盘为 GPT，XP 无法安装；Legacy/MBR 下可能需手动把目标分区设为活动\n"
        )),
    }

    // 5) bootsect /nt52 写 XP 引导码（使引导扇区/MBR 加载 NTLDR）
    let bootsect = bin_dir.join("bootsect.exe");
    if bootsect.exists() {
        let out = new_command(&bootsect)
            .args(["/nt52", win, "/mbr", "/force"])
            .output()
            .map_err(|e| format!("bootsect 执行失败: {e}"))?;
        log.push_str(&gbk_to_utf8(&out.stdout));
        log.push_str(&gbk_to_utf8(&out.stderr));
        if !out.status.success() {
            log.push_str("[bootsect 返回非 0，可能仍可引导]\n");
        }
        log.push_str("已用 bootsect /nt52 写引导码\n");
    } else {
        log.push_str("警告: 未找到 bootsect.exe，未写引导扇区——重启可能无法进入安装\n");
    }

    log.push_str("i386 硬盘文本安装准备完成，重启进入 XP/2003 蓝底文本安装阶段。\n");
    Ok(log)
}

/// 带重试地探测目标卷此刻可写：根目录存在 + 能建/删一个探针目录。
///
/// 应对「刚格式化后盘符短暂卸载/重挂」的瞬时窗口；也能把「所选盘符当前根本没挂」这种情况
/// 转成可读错误，避免后续 `create_dir` 抛裸的 `os error 3`（系统找不到指定的路径）。
fn ensure_volume_ready(win: &str) -> Result<(), String> {
    let root = format!("{win}\\");
    let probe = format!("{win}\\$lr_xp_probe$");
    let mut last = String::from("未知");
    for _ in 0..10 {
        if Path::new(&root).exists() {
            match std::fs::create_dir(&probe) {
                Ok(_) => {
                    let _ = std::fs::remove_dir(&probe);
                    return Ok(());
                }
                // 上次残留的探针目录：能删即视为可写
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    let _ = std::fs::remove_dir(&probe);
                    return Ok(());
                }
                Err(e) => last = e.to_string(),
            }
        } else {
            last = "盘符根目录不存在/未挂载".to_string();
        }
        sleep(Duration::from_millis(500));
    }
    Err(last)
}

/// `create_dir_all` 带几次重试（应对刚格式化后盘符重挂的瞬时窗口）。
fn create_dir_all_retry(path: &str) -> std::io::Result<()> {
    let mut last: Option<std::io::Error> = None;
    for _ in 0..8 {
        match std::fs::create_dir_all(path) {
            Ok(_) => return Ok(()),
            Err(e) => {
                last = Some(e);
                sleep(Duration::from_millis(500));
            }
        }
    }
    Err(last.unwrap_or_else(|| std::io::Error::other("create_dir_all 重试失败")))
}

/// 从 `bin\xp\productkey.txt`（或 `bin\xp_productkey.txt`）读取产品密钥。
///
/// 取第一行非注释（`#`/`;` 开头为注释）、长度足够像密钥（≥20，形如 `XXXXX-XXXXX-XXXXX-XXXXX-XXXXX`）的内容。
/// 没有文件或没有合法行时返回 `None`（→ winnt.sif 用 DefaultHide，仅在密钥页停顿）。
fn read_product_key(bin_dir: &Path) -> Option<String> {
    let candidates = [
        bin_dir.join("xp").join("productkey.txt"),
        bin_dir.join("xp_productkey.txt"),
    ];
    for p in candidates {
        if let Ok(s) = std::fs::read_to_string(&p) {
            for line in s.lines() {
                let t = line.trim();
                if t.is_empty() || t.starts_with('#') || t.starts_with(';') {
                    continue;
                }
                if t.len() >= 20 {
                    return Some(t.to_string());
                }
            }
        }
    }
    None
}

/// 把任意换行规整为 CRLF（winnt.sif 应为 DOS 换行；用户自定义文件可能是 LF）。
fn normalize_crlf(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 16);
    for line in s.split('\n') {
        out.push_str(line.strip_suffix('\r').unwrap_or(line));
        out.push_str("\r\n");
    }
    out
}

/// 用 diskpart 把指定盘符（如 `"C"`）的卷标记为「活动分区」。仅 MBR 有意义。
fn set_volume_active(letter: &str) -> Result<String, String> {
    use std::io::Write;
    let script = format!("select volume {letter}\r\nactive\r\nexit\r\n");
    let tmp = std::env::temp_dir().join("lr_xp_set_active.txt");
    {
        let mut f =
            std::fs::File::create(&tmp).map_err(|e| format!("创建 diskpart 脚本失败: {e}"))?;
        f.write_all(script.as_bytes())
            .map_err(|e| format!("写 diskpart 脚本失败: {e}"))?;
    }
    let tmp_str = tmp.to_string_lossy().into_owned();
    let out = new_command("diskpart")
        .args(["/s", tmp_str.as_str()])
        .output()
        .map_err(|e| format!("diskpart 执行失败: {e}"))?;
    let _ = std::fs::remove_file(&tmp);
    let so = gbk_to_utf8(&out.stdout);
    if !out.status.success() {
        return Err(format!("diskpart 返回非 0: {}", so.trim()));
    }
    Ok(so)
}

/// 强制写入硬盘安装必需的应答键（照搬 DSI 的 `NT5部署无人值守`）。无论用户自定义 .sif 怎么写，
/// 都把这 5 个键设成硬盘安装能跑通的值——尤其 `MsDosInitiated=1`（缺它文本安装会找光盘失败）。
fn force_winnt_keys(content: &str) -> String {
    let mut s = normalize_crlf(content);
    for (section, key, value) in [
        ("[Data]", "MsDosInitiated", "1"),
        ("[Data]", "Floppyless", "1"),
        ("[Data]", "AutoPartition", "0"),
        ("[Data]", "UnattendedInstall", "Yes"),
        ("[Unattended]", "OemPreinstall", "Yes"),
    ] {
        s = set_ini_key(&s, section, key, value);
    }
    s
}

/// 在 INI 文本里把 `section` 节的 `key` 设为 `value`（CRLF）：键存在则替换（并去重），
/// 节存在但缺键则在节内补，节不存在则在文末新建节再补。大小写不敏感匹配节名/键名。
fn set_ini_key(content: &str, section: &str, key: &str, value: &str) -> String {
    let nl = "\r\n";
    let kv = format!("{key}={value}{nl}");
    let mut out = String::with_capacity(content.len() + 64);
    let mut in_target = false;
    let mut inserted = false;
    let mut seen_section = false;
    for line in content.split_inclusive('\n') {
        let t = line.trim();
        let is_header = t.starts_with('[') && t.ends_with(']');
        if is_header {
            if in_target && !inserted {
                out.push_str(&kv);
                inserted = true;
            }
            in_target = t.eq_ignore_ascii_case(section);
            if in_target {
                seen_section = true;
                inserted = false;
            }
            out.push_str(line);
            continue;
        }
        if in_target {
            if let Some((k, _)) = t.split_once('=') {
                if k.trim().eq_ignore_ascii_case(key) {
                    if !inserted {
                        out.push_str(&kv);
                        inserted = true;
                    }
                    continue; // 丢弃原键行/重复键
                }
            }
        }
        out.push_str(line);
    }
    if in_target && !inserted {
        if !out.ends_with('\n') {
            out.push_str(nl);
        }
        out.push_str(&kv);
    } else if !seen_section {
        if !out.ends_with('\n') {
            out.push_str(nl);
        }
        out.push_str(section);
        out.push_str(nl);
        out.push_str(&kv);
    }
    out
}

/// 生成 winnt.sif 应答文件。
///
/// - 有 `product_key`：`UnattendMode=FullUnattended` 全自动（文本+图形全程无停顿）。
/// - 无密钥：`UnattendMode=DefaultHide`（隐藏已答页，仅在「产品密钥」页停一下，其余无人值守）。
///
/// 统一项：跳过 EULA/区域/欢迎；`DriverSigningPolicy=Ignore`（不拦未签名/注入的存储驱动）；
/// 管理员空密码 + 首次自动登录；不分区/不格式化（沿用已格式化的目标盘）；目标 `\WINDOWS`。
/// 出于安全，文本阶段仍由用户确认安装分区（`AutoPartition=0`），避免自动选错盘抹掉数据。
fn winnt_sif(product_key: Option<&str>) -> String {
    let (mode, key_line) = match product_key {
        Some(k) => ("FullUnattended", format!("ProductKey=\"{k}\"\r\n")),
        None => ("DefaultHide", String::new()),
    };
    format!(
        "[Data]\r\n\
AutoPartition=0\r\n\
MsDosInitiated=1\r\n\
UnattendedInstall=Yes\r\n\
Floppyless=1\r\n\
\r\n\
[Unattended]\r\n\
UnattendMode={mode}\r\n\
UnattendSwitch=Yes\r\n\
OemPreinstall=Yes\r\n\
OemSkipEula=Yes\r\n\
TargetPath=\\WINDOWS\r\n\
FileSystem=LeaveAlone\r\n\
WaitForReboot=No\r\n\
DriverSigningPolicy=Ignore\r\n\
\r\n\
[GuiUnattended]\r\n\
AdminPassword=*\r\n\
EncryptedAdminPassword=No\r\n\
AutoLogon=Yes\r\n\
AutoLogonCount=1\r\n\
OEMSkipRegional=1\r\n\
OemSkipWelcome=1\r\n\
TimeZone=210\r\n\
\r\n\
[UserData]\r\n\
FullName=\"User\"\r\n\
OrgName=\"\"\r\n\
ComputerName=*\r\n\
{key_line}\
\r\n\
[Identification]\r\n\
JoinWorkgroup=WORKGROUP\r\n\
\r\n\
[Networking]\r\n\
InstallDefaultComponents=Yes\r\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn winnt_sif_without_key_is_defaulthide() {
        let s = winnt_sif(None);
        assert!(s.contains("UnattendMode=DefaultHide"));
        assert!(!s.contains("ProductKey"));
        assert!(s.contains("DriverSigningPolicy=Ignore"));
        assert!(s.contains("Floppyless=1"));
        assert!(s.contains("OemSkipEula=Yes"));
    }

    #[test]
    fn winnt_sif_with_key_is_fullunattended() {
        let s = winnt_sif(Some("AAAAA-BBBBB-CCCCC-DDDDD-EEEEE"));
        assert!(s.contains("UnattendMode=FullUnattended"));
        assert!(s.contains("ProductKey=\"AAAAA-BBBBB-CCCCC-DDDDD-EEEEE\""));
    }

    #[test]
    fn force_keys_overrides_msdosinitiated() {
        // 用户自定义 .sif 里 MsDosInitiated="0" → 必须被强制改成 1（照搬 DSI）
        let input = ";c\r\n[Data]\r\n    AutoPartition=1\r\n    MsDosInitiated=\"0\"\r\n    UnattendedInstall=\"Yes\"\r\n\r\n[Unattended]\r\n    OemPreinstall=No\r\n    TargetPath=\\WINDOWS\r\n";
        let out = force_winnt_keys(input);
        assert!(out.contains("MsDosInitiated=1"));
        assert!(!out.contains("MsDosInitiated=\"0\""));
        assert!(out.contains("AutoPartition=0") && !out.contains("AutoPartition=1"));
        assert!(out.contains("OemPreinstall=Yes") && !out.contains("OemPreinstall=No"));
        assert!(out.contains("Floppyless=1")); // 原本缺 → 补进 [Data]
        assert!(out.contains("TargetPath=\\WINDOWS")); // 无关行保留
    }

    #[test]
    fn set_ini_key_creates_missing_section() {
        let out = set_ini_key("[Foo]\r\nx=1\r\n", "[Data]", "MsDosInitiated", "1");
        assert!(out.contains("[Data]\r\nMsDosInitiated=1\r\n"));
    }

    #[test]
    fn set_ini_key_dedups_existing_key() {
        let out = set_ini_key("[Data]\r\nk=a\r\nk=b\r\n", "[Data]", "k", "z");
        assert_eq!(out.matches("k=").count(), 1);
        assert!(out.contains("k=z"));
    }

    #[test]
    fn nt5_bootfiles_parses_manifest() {
        let v: Vec<&str> = nt5_bootfiles().collect();
        assert!(v.contains(&"ATAPI.SY_"));
        assert!(v.contains(&"NTKRNLMP.EX_"));
        assert!(v.contains(&"TXTSETUP.SIF"));
        assert!(v.iter().all(|l| !l.starts_with('#') && !l.is_empty()));
        assert!(v.len() > 100);
    }

    #[test]
    fn normalize_crlf_converts_lf_and_keeps_crlf() {
        assert_eq!(normalize_crlf("a\nb"), "a\r\nb\r\n");
        assert_eq!(normalize_crlf("a\r\nb\r\n"), "a\r\nb\r\n\r\n");
        assert_eq!(normalize_crlf("[Data]\nAutoPartition=0"), "[Data]\r\nAutoPartition=0\r\n");
    }

    #[test]
    fn winnt_sif_baseline_has_msdosinitiated_1() {
        // 基线生成的应答即应是 1（force 再保险一次）
        let s = winnt_sif(None);
        assert!(s.contains("MsDosInitiated=1"));
        assert!(s.contains("UnattendSwitch=Yes"));
        assert!(s.contains("FileSystem=LeaveAlone"));
        assert!(force_winnt_keys(&s).contains("MsDosInitiated=1"));
    }
}
