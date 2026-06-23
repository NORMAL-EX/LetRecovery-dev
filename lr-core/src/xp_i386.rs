//! Windows XP / 2003 的「i386 硬盘文本模式安装」（仅 Legacy/MBR）。
//!
//! 参考 WinNTSetup 等成熟工具的「无光驱硬盘装 XP」做法实现。原版 XP/2003 安装盘是 `\I386`
//! 文本安装结构，没有 Vista+ 的 `\sources\install.wim`，无法像 Win7+ 那样「释放 WIM」。
//! 经典流程：
//!
//!   1. 把 `i386` 整个复制到 目标盘 `\$WIN_NT$.~LS\I386`（文本安装阶段的本地源）；
//!   2. 用 `setupldr.bin` 充当根目录 `NTLDR`（开机直接进入「文本安装」）；
//!   3. 复制 `ntdetect.com`（以及源里若有的 `biosinfo.inf` / `bootfont.bin`）到根；把
//!      `txtsetup.sif` 放根并把 `[SetupData] SetupSourcePath` 指向 `\$WIN_NT$.~LS\`，
//!      让文本安装从本地源取文件；
//!   4. 写入 `winnt.sif` 应答（尽量全无人值守：跳过 EULA/区域/欢迎、忽略驱动签名、
//!      管理员空密码、首次自动登录；放了产品密钥则全自动，没放则只在「密钥」页停一下）；
//!   5. 标记目标分区为「活动分区」+ `bootsect /nt52` 写 XP 引导码（MBR/引导扇区加载 NTLDR）。
//!
//! 重启后 → XP/2003 文本安装（蓝底）→ 复制文件 → 再次重启 → 图形安装。
//!
//! 限制：**仅 Legacy/BIOS + MBR**。XP 不支持 GPT/UEFI（调用方在 UI 已拦截 GPT/UEFI 目标）。
//! 调用前目标盘需已格式化(NTFS/FAT32)、且应为目标磁盘上的主分区。

use std::path::Path;
use std::thread::sleep;
use std::time::Duration;

use crate::command::new_command;
use crate::encoding::gbk_to_utf8;

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
pub fn install_from_i386(
    i386_src: &Path,
    win_partition: &str,
    bin_dir: &Path,
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

    // 1) 复制 i386 → win\$WIN_NT$.~LS\I386（文本安装本地源）
    let ls_i386 = format!("{win}\\$WIN_NT$.~LS\\I386");
    log.push_str(&format!("复制 i386 到 {ls_i386} ...\n"));
    create_dir_all_retry(&ls_i386).map_err(|e| format!("创建 {ls_i386} 失败: {e}"))?;
    let src = i386_src.to_string_lossy().to_string();
    let out = new_command("xcopy")
        .args([src.as_str(), ls_i386.as_str(), "/E", "/I", "/H", "/R", "/Y", "/Q"])
        .output()
        .map_err(|e| format!("xcopy 执行失败: {e}"))?;
    log.push_str(&gbk_to_utf8(&out.stdout));
    if !out.status.success() {
        return Err(format!("复制 i386 失败:\n{}", gbk_to_utf8(&out.stderr)));
    }

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

    // 3) txtsetup.sif -> 根，[SetupData] SetupSourcePath 指向 \$WIN_NT$.~LS\
    let raw = std::fs::read(&txtsetup).map_err(|e| format!("读 txtsetup.sif 失败: {e}"))?;
    let txt = match String::from_utf8(raw.clone()) {
        Ok(s) => s,
        Err(_) => gbk_to_utf8(&raw),
    };
    let patched = patch_txtsetup(&txt);
    std::fs::write(format!("{win}\\txtsetup.sif"), patched.as_bytes())
        .map_err(|e| format!("写 txtsetup.sif 失败: {e}"))?;
    log.push_str("已写入 txtsetup.sif（SetupSourcePath=\\$WIN_NT$.~LS\\）\n");

    // 4) winnt.sif 应答（尽量全无人值守；有产品密钥则全自动）
    let product_key = read_product_key(bin_dir);
    match &product_key {
        Some(_) => log.push_str("检测到产品密钥（bin\\xp\\productkey.txt）→ 全自动无人值守\n"),
        None => log.push_str(
            "未提供产品密钥（可放 bin\\xp\\productkey.txt 实现全自动）→ 仅「密钥」页停顿，其余无人值守\n",
        ),
    }
    std::fs::write(
        format!("{win}\\winnt.sif"),
        winnt_sif(product_key.as_deref()).as_bytes(),
    )
    .map_err(|e| format!("写 winnt.sif 失败: {e}"))?;
    log.push_str("已写入 winnt.sif 应答文件\n");

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

/// 把 txtsetup.sif 的 `[SetupData]` 节里 `SetupSourcePath` 设为 `"\$WIN_NT$.~LS\"`。
fn patch_txtsetup(content: &str) -> String {
    let mut out = String::new();
    let mut in_setupdata = false;
    let mut wrote = false;
    const LINE: &str = "SetupSourcePath = \"\\$WIN_NT$.~LS\\\"\r\n";
    for line in content.lines() {
        let t = line.trim();
        if t.starts_with('[') && t.ends_with(']') {
            if in_setupdata && !wrote {
                out.push_str(LINE);
                wrote = true;
            }
            in_setupdata = t.eq_ignore_ascii_case("[SetupData]");
            out.push_str(line);
            out.push_str("\r\n");
            continue;
        }
        if in_setupdata && t.to_ascii_lowercase().starts_with("setupsourcepath") {
            out.push_str(LINE);
            wrote = true;
            continue;
        }
        out.push_str(line);
        out.push_str("\r\n");
    }
    if in_setupdata && !wrote {
        out.push_str(LINE);
    }
    // 若整份文件没有 [SetupData] 节，补一个（极少见）
    if !out.to_ascii_lowercase().contains("[setupdata]") {
        out.push_str("\r\n[SetupData]\r\n");
        out.push_str(LINE);
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
MsDosInitiated=0\r\n\
UnattendedInstall=Yes\r\n\
Floppyless=1\r\n\
\r\n\
[Unattended]\r\n\
UnattendMode={mode}\r\n\
OemPreinstall=No\r\n\
OemSkipEula=Yes\r\n\
TargetPath=\\WINDOWS\r\n\
FileSystem=*\r\n\
Repartition=No\r\n\
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
    fn patch_txtsetup_inserts_sourcepath_in_setupdata() {
        let input = "[SetupData]\r\nSetupSourcePath = \"\\\"\r\nBootPath = \"\\$WIN_NT$.~BT\"\r\n";
        let out = patch_txtsetup(input);
        assert!(out.contains("SetupSourcePath = \"\\$WIN_NT$.~LS\\\""));
        // 原 BootPath 行保留
        assert!(out.contains("BootPath = \"\\$WIN_NT$.~BT\""));
    }

    #[test]
    fn patch_txtsetup_adds_setupdata_when_missing() {
        let input = "[SourceDisksNames]\r\n1 = foo\r\n";
        let out = patch_txtsetup(input);
        assert!(out.to_ascii_lowercase().contains("[setupdata]"));
        assert!(out.contains("SetupSourcePath = \"\\$WIN_NT$.~LS\\\""));
    }
}
