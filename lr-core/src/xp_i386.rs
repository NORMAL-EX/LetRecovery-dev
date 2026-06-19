//! Windows XP / 2003 的「i386 硬盘文本模式安装」（仅 Legacy/MBR）。
//!
//! 背景：原版 XP/2003 安装盘里是 `\I386` 文本安装结构，没有 Vista+ 的 `\sources\install.wim`，
//! 因此无法像 Win7+ 那样「释放 WIM」。本模块实现经典的「无光驱硬盘装 XP」流程：
//!
//!   1. 把 `i386` 整个复制到 目标盘 `\$WIN_NT$.~LS\I386`（文本安装阶段的本地源）；
//!   2. 用 `setupldr.bin` 充当根目录 `NTLDR`（开机直接进入「文本安装」）；
//!   3. 复制 `ntdetect.com`；把 `txtsetup.sif` 放到根目录并把 `[SetupData] SetupSourcePath`
//!      指向 `\$WIN_NT$.~LS\`，让文本安装从本地源取文件；
//!   4. 写入 `winnt.sif` 应答（跳过 EULA/区域/欢迎，目标 `\WINDOWS`，不重新分区）；
//!   5. `bootsect /nt52` 写 XP 引导码，使 MBR/引导扇区加载 NTLDR。
//!
//! 重启后 → XP 文本安装（蓝底）→ 复制文件 → 再次重启 → 图形安装。
//!
//! 限制：**仅 Legacy/BIOS + MBR**。XP 不支持 GPT/UEFI。调用前目标盘需已格式化(NTFS/FAT32)、为活动分区。

use std::path::Path;

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
/// - `i386_src`：i386 目录（如挂载 ISO 的 `H:\I386`，或已复制到数据分区的副本）。
/// - `win_partition`：目标系统盘（如 `"C:"`），需已格式化且为活动分区。
/// - `bin_dir`：程序 bin 目录（取 `bootsect.exe`）。
pub fn install_from_i386(
    i386_src: &Path,
    win_partition: &str,
    bin_dir: &Path,
) -> Result<String, String> {
    let win = win_partition.trim_end_matches('\\'); // "C:"
    let mut log = String::new();

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

    // 1) 复制 i386 → win\$WIN_NT$.~LS\I386
    let ls_i386 = format!("{}\\$WIN_NT$.~LS\\I386", win);
    log.push_str(&format!("复制 i386 到 {} ...\n", ls_i386));
    std::fs::create_dir_all(&ls_i386).map_err(|e| format!("创建 {} 失败: {}", ls_i386, e))?;
    let src = i386_src.to_string_lossy().to_string();
    let out = new_command("xcopy")
        .args([src.as_str(), ls_i386.as_str(), "/E", "/I", "/H", "/R", "/Y", "/Q"])
        .output()
        .map_err(|e| format!("xcopy 执行失败: {}", e))?;
    log.push_str(&gbk_to_utf8(&out.stdout));
    if !out.status.success() {
        return Err(format!("复制 i386 失败:\n{}", gbk_to_utf8(&out.stderr)));
    }

    // 2) setupldr.bin -> 根\NTLDR；ntdetect.com -> 根
    std::fs::copy(&setupldr, format!("{}\\NTLDR", win))
        .map_err(|e| format!("写 NTLDR 失败: {}", e))?;
    std::fs::copy(&ntdetect, format!("{}\\NTDETECT.COM", win))
        .map_err(|e| format!("写 NTDETECT.COM 失败: {}", e))?;
    log.push_str("已写入 NTLDR(setupldr) / NTDETECT.COM\n");

    // 3) txtsetup.sif -> 根，[SetupData] SetupSourcePath 指向 \$WIN_NT$.~LS\
    let raw = std::fs::read(&txtsetup).map_err(|e| format!("读 txtsetup.sif 失败: {}", e))?;
    let txt = match String::from_utf8(raw.clone()) {
        Ok(s) => s,
        Err(_) => gbk_to_utf8(&raw),
    };
    let patched = patch_txtsetup(&txt);
    std::fs::write(format!("{}\\txtsetup.sif", win), patched.as_bytes())
        .map_err(|e| format!("写 txtsetup.sif 失败: {}", e))?;
    log.push_str("已写入 txtsetup.sif（SetupSourcePath=\\$WIN_NT$.~LS\\）\n");

    // 4) winnt.sif 应答
    std::fs::write(format!("{}\\winnt.sif", win), winnt_sif().as_bytes())
        .map_err(|e| format!("写 winnt.sif 失败: {}", e))?;
    log.push_str("已写入 winnt.sif 应答文件\n");

    // 5) bootsect /nt52 写 XP 引导码（使引导扇区/MBR 加载 NTLDR）
    let bootsect = bin_dir.join("bootsect.exe");
    if bootsect.exists() {
        let out = new_command(&bootsect)
            .args(["/nt52", win, "/mbr", "/force"])
            .output()
            .map_err(|e| format!("bootsect 执行失败: {}", e))?;
        log.push_str(&gbk_to_utf8(&out.stdout));
        log.push_str(&gbk_to_utf8(&out.stderr));
        if !out.status.success() {
            log.push_str("[bootsect 返回非 0，可能仍可引导]\n");
        }
        log.push_str("已用 bootsect /nt52 写引导码\n");
    } else {
        log.push_str("警告: 未找到 bootsect.exe，未写引导扇区——重启可能无法进入安装\n");
    }

    log.push_str("i386 硬盘文本安装准备完成，重启进入 XP 文本安装阶段。\n");
    Ok(log)
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

/// 生成 winnt.sif 应答文件（半无人值守：跳过 EULA/区域/欢迎；不分区/不格式化；目标 \WINDOWS）。
/// 不内置产品密钥——文本阶段照常进行，图形阶段如需密钥会提示输入。
fn winnt_sif() -> String {
    "[Data]\r\n\
AutoPartition=0\r\n\
MsDosInitiated=0\r\n\
UnattendedInstall=Yes\r\n\
\r\n\
[Unattended]\r\n\
UnattendMode=ProvideDefault\r\n\
OemSkipEula=Yes\r\n\
TargetPath=\\WINDOWS\r\n\
FileSystem=*\r\n\
Repartition=No\r\n\
WaitForReboot=No\r\n\
\r\n\
[GuiUnattended]\r\n\
OEMSkipRegional=1\r\n\
OemSkipWelcome=1\r\n\
TimeZone=210\r\n\
\r\n\
[UserData]\r\n\
FullName=\"User\"\r\n\
OrgName=\"\"\r\n\
ComputerName=*\r\n\
\r\n\
[Identification]\r\n\
JoinWorkgroup=WORKGROUP\r\n\
\r\n\
[Networking]\r\n\
InstallDefaultComponents=Yes\r\n"
        .to_string()
}
