# Moment

本地优先的屏幕时间 / 应用使用时间统计工具。界面参考 iOS / macOS Screen Time，追求**低资源占用**与长时间后台稳定运行。

## 特性

- **事件驱动，近零 CPU**：Wayland 下通过 `wlr-foreign-toplevel` + `ext-idle-notify` 协议实时接收前台应用切换与空闲事件，无轮询、无忙等；X11 下以 1s 低频轮询兜底
- **低频落盘**：会话在内存中聚合，应用切换 / 每 10s checkpoint / 退出时才写 SQLite（空闲时零写入）
- **空闲检测**：无输入超过阈值即停止计时，空闲时段单独记录（`idle_periods` 表）
- **异常防护**：单调钟钳制——系统休眠、时钟跳变不会产生虚长会话；崩溃遗留会话启动时自动回收
- **屏幕时间概览**：今天 / 近 7 天、周趋势、逐时活动、应用排行与自动分类；应用页提供单应用 30 天趋势
- **托盘常驻**：关闭窗口即最小化到托盘继续统计；支持暂停 / 恢复、开机自启
- **深色 / 浅色主题**：一键切换，重启保持
- **本地优先**：数据只存在本机 SQLite（Linux：`~/.local/share/moment/moment.db`；Windows：`%LOCALAPPDATA%\Moment\moment.db`），无账号、无云端、无遥测

## 技术栈

Rust · Tauri 2 · React 19 + TypeScript · Vite · Tailwind CSS 4 · SQLite (rusqlite, WAL)

```
src-tauri/src/
  platform/    # OS 抽象层 (trait PlatformTracker): wayland.rs / x11.rs / icon.rs
  tracker/     # 追踪引擎: 会话状态机 + checkpoint + 单调钟防护
  database/    # SQLite 持久层: schema.rs (版本化迁移) / repository.rs
  commands/    # Tauri command 薄壳
src/
  pages/       # 概览 / 应用 / 设置
  components/  # Sidebar / AppIcon
  design/      # 颜色、间距、字体与圆角 tokens
  lib/api.ts   # 类型化 invoke
```

架构与设计决策详见 [docs/DESIGN.md](docs/DESIGN.md)。

## 开发

```bash
npm install
npm run tauri dev      # 开发模式
npm run tauri build    # 产出 release 二进制 + deb

# 在 Linux 宿主上对 Windows 平台代码做类型检查 (仅 check, 不链接):
cd src-tauri && RUSTFLAGS='--cfg xcheck' cargo check
```

## 安装与使用

从 [GitHub Releases](https://github.com/Hexarchh/moment/releases) 下载对应系统的安装包：

- **Windows 10/11 x64**：下载 `Moment_*_x64-setup.exe` 或 `Moment_*_x64_en-US.msi`，双击安装后从开始菜单启动。
- **Debian / Ubuntu x64**：下载 `Moment_*_amd64.deb`，执行 `sudo apt install ./Moment_*_amd64.deb`，然后从应用菜单启动或运行 `moment`。

窗口关闭后仍在系统托盘运行；托盘菜单可重新打开、暂停统计或彻底退出。设置页可开启开机自启。升级时先从托盘退出旧进程，再运行新安装包。

## 平台支持

- Linux (Wayland) ✅ 完整实现并实测
- Linux (X11/XWayland) ✅ 兜底实现
- Windows ✅ 已实现并在 Windows CI 编译、运行单测；前台统计仍待实际桌面环境验证
- macOS ⬜ 计划中（预留 `PlatformTracker` 扩展位）

## 许可

[MIT](LICENSE) © 2026 Chen Kai
