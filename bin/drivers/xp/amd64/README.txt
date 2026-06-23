============================================================
  XP / 2003【64 位 (amd64)】文本安装 —— 存储驱动目录
============================================================

装【64 位 XP x64 / Server 2003 x64 的 AMD64 镜像】(光盘根目录 \AMD64)时，文本
安装(蓝底)阶段需要的【64 位】存储驱动放到这里，LetRecovery 会自动集成进文本安装。

★ 随包自带、自动生效(无需你手动放)：
    bin\drivers\xp\ahci\   —— 通用 AHCI(genahci，PCI\CC_010601)
    bin\drivers\xp\nvme\   —— 标准 NVMe(stornvme，PCI\CC_010802)
  装 64 位 XP/2003 文本安装时，这两个魔改驱动会被自动集成。

想补充更多 64 位驱动就放这里：
  每个驱动一个子文件夹，内含 .inf + .sys(+ .cat)，例如：
      bin\drivers\xp\amd64\mynvme\
          mynvme.inf
          mynvme.sys
  会被递归扫描。工具据 .inf(服务名 / 硬件 ID PCI\... / .sys)合并进 txtsetup.sif。

注意：
  ★ 必须是【64 位 (amd64)】驱动；32 位的放 bin\drivers\xp\x86\。
  - .inf 信息不全的目录会被静默跳过，不影响安装。
  - 这套只对走「i386 引擎」的 \AMD64 文本安装介质生效；走「UEFI 化 XP x64 WIM」那条
    路是另一套离线注入(inject_xp_drivers)，用的也是 bin\drivers\xp\ 这些驱动。
