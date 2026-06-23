# 原版 Windows XP / 2003（i386 介质）硬盘文本安装

针对**原版 XP/2003 安装盘**（根目录是 `\I386`、没有 `\sources\install.wim` 的那种）。
这种介质无法像 Win7+ 那样「释放 WIM」，LetRecovery 参考 WinNTSetup 等成熟工具的
「无光驱硬盘装 XP」做法，实现于 `lr-core/src/xp_i386.rs`（`install_from_i386`）。

## 识别

挂载 ISO 后找不到 `\sources\install.wim/esd`，但存在 `\I386\setupldr.bin` 等三件套，
即识别为 XP/2003 i386 文本安装介质（`iso::xp_i386_dir` / `is_valid_i386`），UI 显示绿色
「已识别为 Windows XP/2003 i386 文本安装介质」。

## 准备流程（重启前在 PE/当前系统里做）

目标盘 `WIN`（如 `C:`），来源 `i386_src`（如挂载盘 `G:\I386`）：

1. **可写探测（带重试）**：先确认 `WIN\` 此刻真的可建目录。刚格式化完盘符可能短暂
   卸载/重挂，过去会在下一步 `create_dir` 抛裸 `os error 3（系统找不到指定的路径）`；
   现在带 ~5s 重试，过不了就给出可读原因（盘符未挂载 / 非 NTFS / GPT 等）。
2. **本地源**：`xcopy i386` → `WIN\$WIN_NT$.~LS\I386`。
3. **根引导文件**：`setupldr.bin` → `WIN\NTLDR`（开机直接进文本安装）；
   `ntdetect.com` → `WIN\NTDETECT.COM`；源里若有 `biosinfo.inf` / `bootfont.bin` 一并落根。
4. **txtsetup.sif**：拷到 `WIN\txtsetup.sif`，把 `[SetupData] SetupSourcePath` 改成
   `"\$WIN_NT$.~LS\"`，让文本安装从本地源取文件。
5. **winnt.sif 应答**（见下）→ `WIN\winnt.sif`。
6. **置活动分区**（diskpart `active`）+ **`bootsect /nt52 WIN /mbr /force`** 写 XP 引导码。

重启 → 蓝底文本安装 → 复制文件 → 再次重启 → 图形安装。

## 无人值守（winnt.sif）

| 是否放 `bin\xp\productkey.txt` | UnattendMode | 行为 |
|---|---|---|
| 否（默认） | `DefaultHide` | 跳过 EULA/区域/欢迎、忽略驱动签名、管理员空密码+首次自动登录；图形阶段**只在「产品密钥」页停一下**，其余全自动 |
| 是 | `FullUnattended` | 全程不停顿，真·全自动 |

统一项：`Floppyless=1`、`OemSkipEula=Yes`、`DriverSigningPolicy=Ignore`（不拦未签名/注入的
存储驱动）、`TargetPath=\WINDOWS`、`Repartition=No`、`FileSystem=*`（沿用已格式化的盘，不重新分区/格式化）。

> 工具**不内置产品密钥**。要全自动就自己在 `bin\xp\productkey.txt` 放一行与所装版本/渠道匹配的密钥。
> 出于安全，文本阶段仍由用户确认「装到哪个分区」（`AutoPartition=0`），不自动选盘以免抹错盘。

## 边界 / 已知限制

- **仅 Legacy/BIOS + MBR**。XP 不支持 GPT/UEFI——调用方在发起安装时已拦截 GPT/UEFI 目标。
- 不支持在「正在运行的系统盘」上原地安装，需先进 PE。
- 暂未做**文本阶段大容量存储驱动集成**（把 NVMe/AHCI 驱动并入 txtsetup.sif）。若目标机的
  系统盘挂在原版 XP 不自带驱动的控制器上（如 NVMe），文本阶段可能找不到硬盘——这类机器
  请改用「已 UEFI 化的 XP x64 WIM 镜像」路径（见 `docs/xp-gpt-uefi.md`）。

## 改动文件

- `lr-core/src/xp_i386.rs`：重写 `install_from_i386`（可写探测+重试、完整根引导文件、
  健壮 `winnt.sif`、可选产品密钥）；新增单元测试。
- `正常系统端/src/ui/install_progress.rs`：修复 `format_partition` 的无效 `/Y` 开关
  （改管道确认，真正完成格式化）。
- `bin/xp/README.txt`：可选产品密钥说明；`.github/workflows/build-and-release.yml`：打包 `bin/xp/`。
