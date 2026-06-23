============================================================
  XP / 2003【32 位 (i386)】文本安装 —— 存储驱动目录
============================================================

装【原版 32 位 XP / 2003 的 i386 镜像】(光盘根目录 \I386)时，如果文本安装
(蓝底)阶段找不到硬盘(常见于 AHCI / NVMe 控制器)，把对应的【32 位】存储驱动
放到这里，LetRecovery 会在安装时自动集成进文本安装。

怎么放：
  每个驱动一个子文件夹，里面放它的 .inf + .sys(+ .cat)。例如：
      bin\drivers\xp\x86\myahci\
          myahci.inf
          myahci.sys
  支持多层子目录、多个驱动，会被递归扫描。

工具会做什么(nLite / WinNTSetup 那一套)：
  - 解析每个 .inf：取服务名、miniport 的 .sys、硬件 ID(PCI\...)；
  - 把该目录里所有 .sys 拷进文本安装本地源；
  - 往 txtsetup.sif 写 [SourceDisksFiles] / [SCSI.Load] / [SCSI] / [HardwareIdsDatabase]。
  这样文本阶段就能加载驱动认盘，并把服务登记进装好的系统。

注意：
  ★ 必须是【32 位 (i386/x86)】驱动。64 位(amd64)的 .sys 装不进 32 位 XP——那种放
     bin\drivers\xp\amd64\(装 64 位 XP/2003 时才用)。
  - .inf 信息不全(没服务名 / 没硬件 ID / 没 .sys)的目录会被静默跳过，不影响安装。
  - 现实提醒：32 位 XP 的现代 NVMe 驱动基本没有；多数 SATA 机器把 BIOS 的 SATA 模式
    切成 IDE/Compatibility 就能直接装，不需要这里的驱动。
