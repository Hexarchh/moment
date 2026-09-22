# Moment

本地优先的屏幕时间 / 应用使用时间统计工具。风格参考 macOS Screen Time 与 Linear，追求**极低资源占用**与长时间后台稳定运行。

## 特性

- **事件驱动，近零 CPU**：Wayland 下通过 `wlr-foreign-toplevel` + `ext-idle-notify` 协议实时接收前台应用切换与空闲事件，无轮询、无忙等；X11 下以 1s 低频轮询兜底
- **低频落盘**：会话在内存中聚合，应用切换 / 每 10s checkpoint / 退出时才写 SQLite（空闲时零写入）
- **空闲检测**：无输入超过阈值即停止计时，空闲时段单独记录（`idle_periods` 表）
- **异常防护**：单调钟钳制——系统休眠、时钟跳变不会产生虚长会话；崩溃遗留会话启动时自动回收
- **完整统计**：今日 / 近 7 天 / 近 30 天趋势、较昨日对比、应用排行（真实图标 + 占比）、单应用 30 天趋势
- **托盘常驻**：关闭窗口即最小化到托盘继续统计；支持暂停 / 恢复
- **本地优先**：数据只存在本机 SQLite（`~/.local/share/moment/moment.db`），无账号、无云端、无遥测

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
  lib/api.ts   # 类型化 invoke
```

架构与设计决策详见 [docs/DESIGN.md](docs/DESIGN.md)。

## 开发

```bash
npm install
npm run tauri dev      # 开发模式
npm run tauri build    # 产出 release 二进制 + deb
```

## 平台支持

- Linux (Wayland) ✅ 完整实现
- Linux (X11/XWayland) ✅ 兜底实现
- Windows / macOS ⬜ 计划中（沿用 `PlatformTracker` trait 逐平台扩展）

## 许可

个人项目，未指定开源协议。
