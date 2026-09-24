# Moment 设计文档

本地优先的屏幕时间统计工具。优先级：稳定性 > 资源占用 > 统计准确性 > UI 体验 > 功能数量。

状态标注：✅ 已实现并验证 · 🔶 部分实现 · ⬜ 未实现（P2 计划）

---

## ① 项目整体架构

四层解耦，统计逻辑与 UI 完全隔离。数据单向流动：

```
┌─────────────────────────────────────────────────────┐
│ 前端 React (展示层)                                    │
│   pages/ Overview·Apps·Settings                       │
│   仅在页面挂载/窗口聚焦时 invoke，无高频定时器            │
└──────────────┬──────────────────────────────────────┘
               │ invoke (低频: 20s 轮询 + focus 事件)
┌──────────────▼──────────────────────────────────────┐
│ commands (薄壳层)                                     │
│   overview / timeline / apps / get_settings / …       │
│   只做参数解析 + 转发，无业务逻辑                        │
└──────────────┬──────────────────────────────────────┘
               │ DbHandle (Arc<Mutex<Connection>>)
┌──────────────▼──────────────────────────────────────┐
│ database (持久层)                                     │
│   schema.rs 版本化迁移 · repository.rs 读写与统计查询   │
└──────────────▲──────────────────────────────────────┘
               │ 会话 open / checkpoint / close
┌──────────────┴──────────────────────────────────────┐
│ tracker (引擎层, 独立线程)                             │
│   会话状态机: 切窗/空闲/暂停 → 结算会话                  │
│   10s checkpoint · 崩溃孤儿会话回收                    │
└──────────────▲──────────────────────────────────────┘
               │ TrackerEvent (事件流)
┌──────────────┴──────────────────────────────────────┐
│ platform (OS 抽象层, trait PlatformTracker)           │
│   wayland.rs 事件驱动 · x11.rs 低频轮询兜底             │
│   windows.rs / macos.rs 后续按同 trait 增加            │
└─────────────────────────────────────────────────────┘
```

目录结构（实际代码）：

```
src-tauri/src/
  lib.rs            # 应用装配: 托盘/窗口隐藏/引擎启动/优雅退出
  state.rs          # AppState: 暂停标志 + DbHandle + TrackerHandle
  error.rs          # AppError (thiserror + Serialize)
  platform/
    mod.rs          # trait PlatformTracker + create_tracker 工厂
    wayland.rs      # wlr-foreign-toplevel + ext-idle-notify ✅
    x11.rs          # _NET_ACTIVE_WINDOW + MIT-SCREEN-SAVER ✅
  tracker/mod.rs    # Engine 状态机 + 线程循环 + 单元测试 ✅
  database/
    mod.rs          # Db (WAL, 短临界区) ✅
    schema.rs       # 版本化迁移 ✅
    repository.rs   # 读写 + 统计查询 + 单元测试 ✅
  models/mod.rs     # AppUsage / SessionDetail / Settings ✅
  commands/mod.rs   # Tauri command 薄壳 ✅
src/
  lib/api.ts        # 类型化 invoke + 格式化工具 ✅
  pages/            # Overview / Apps / Settings ✅
  components/       # Sidebar ✅
  design/           # colors / spacing / typography / radius ✅
  styles/global.css # 主题变量与组件样式 ✅
```

## ② 技术选型及理由

| 选型 | 决定 | 理由 |
|---|---|---|
| 框架 | Tauri 2 | 复用系统 WebView，无 Chromium 打包；bundle 小、启动快 |
| 前端 | **保留 React 19**（不用 Svelte） | 见下「React vs Svelte」 |
| 数据库 | rusqlite (bundled) | 静态编译系统无关的 SQLite，WAL 单写多读；无 ORM，查询直写 SQL |
| 异步运行时 | **不用 Tokio** | 见 ⑦；全 std 线程 + 阻塞 IO |
| wayland | wayland-client + wlr/ext 协议 | 纯事件驱动，零轮询 |
| x11 | x11rb | 无 unsafe C 绑定，EWMH 标准属性 |
| 日志 | tracing，release 默认 warn | 调试期 debug，生产静默 |

**React vs Svelte**：本项目内存大头是 WebKitGTK 进程本身（60–100MB 级），React 运行时相对 Svelte 的差值（几 MB）不构成换栈理由；且前端是纯展示、无高频更新，React 的调度开销无从体现。后端已通过 command 层完全解耦，未来若要极限压缩，前端可整体替换而不动 Rust 侧。故保留 React，避免重写已验证的 UI。

## ③ 屏幕时间统计的底层原理

核心是**事件驱动的会话化（sessionization）**：不问「现在前台是什么」N 次，而是让 OS 在「前台变化」时刻推事件，引擎维护「当前会话」状态：

1. **前台窗口检测**
   - Linux/Wayland：`zwlr_foreign_toplevel_manager_v1` — compositor 维护全部 toplevel 列表，`state` 数组含 activated 位；绑定后即得初始列表，此后激活变化实时推送。标题变化也会推送（用于会话标题）。
   - Linux/X11：EWMH `_NET_ACTIVE_WINDOW` 根窗口属性 + `WM_CLASS`/`_NET_WM_PID`（→ `/proc/pid/exe` 取可执行路径）。无事件订阅的轻量方案下用 1s 轮询 + 去重。
   - Windows（规划）：`SetWinEventHook(EVENT_SYSTEM_FOREGROUND)` 事件 + `GetForegroundWindow`；可执行名经 `QueryFullProcessImageName`。
   - macOS（规划）：`NSWorkspace.didActivateApplication` 通知 + `frontmostApplication`。
2. **空闲检测**
   - Wayland：`ext_idle_notifier_v1` 按 seat 注册阈值，到点推 `Idled`/`Resumed`——引擎零轮询。
   - X11：MIT-SCREEN-SAVER 扩展 `QueryInfo` 返回距上次输入毫秒数，与阈值比较。
   - Windows：`GetLastInputInfo`；macOS：`CGEventSource.secondsSinceLastEventType`。
3. **会话规则**（引擎状态机，已实现并有单测覆盖）
   - 前台 app 变化 → 结算旧会话（ended_at=now），开新会话
   - 同 app 标题变化 → 不切分，标题在下次 checkpoint 落盘（避免终端/编辑器动态标题造成会话碎片）
   - 空闲开始 → 会话截断到「最后一次输入时刻」（now − idle_threshold，clamp 不早于 started_at）
   - 空闲恢复/取消暂停 → 平台层补报当前前台窗口，立即重启会话（不等下一次真实切窗）
   - 空闲期间收到的窗口事件只更新内存状态不上报（后台应用的标题刷新不重启计时）
4. **防重复/防脏数据**：checkpoint 机制保证会话在 DB 中始终有 ended_at（滚动更新）；进程被杀最多丢 ≤10s 数据，不会产生「40 小时会话」（ended_at 永远 ≤ now 且 ≥ started_at，启动时孤儿会话按 started_at 截断）。

## ④ 数据流设计

写路径（引擎线程独占，内存聚合，低频落盘）：

```
TrackerEvent ──▶ Engine 状态机 ──▶ OpenSession(内存: id/app_key/title/started_at/dirty)
   切窗: UPDATE 旧会话 ended_at + INSERT 新会话 + UPSERT application   (2 次写)
   标题变: 仅内存 dirty 标记                                          (0 次写)
   空闲/暂停: UPDATE ended_at (截断到最后输入时刻)                      (1 次写)
   checkpoint (每 10s): UPDATE ended_at=now [, title]                  (1 次写)
   退出: UPDATE ended_at                                               (1 次写)
```

即稳态下**每 10 秒一次单行 UPDATE**，空闲时**零写入**。

读路径（command → SQL，本地日期 [0点, 次日0点) 转 UTC 后按 RFC3339 字典序比较；会话与查询窗相交时用 `julianday(MIN(ended_at,?2)) − julianday(MAX(started_at,?1))` 裁剪，跨午夜会话正确分摊到两天）：

- 今日总时长 / 近 7 天逐日 → `SUM` over `idx_sessions_started`
- 应用排行 → `GROUP BY application_id ORDER BY total DESC`
- 时间线明细 → join applications 按 started_at 排序

## ⑤ SQLite Schema（已实现）

```sql
applications(id PK, app_key TEXT UNIQUE, name, executable, path, first_seen)
  -- app_key = 归一化标识 (wayland app_id / x11 可执行名)，UPSERT 去重

sessions(id PK, application_id REFERENCES applications(id),
         title, started_at TEXT, ended_at TEXT)          -- ended_at NULL = 进行中
CREATE INDEX idx_sessions_started  ON sessions(started_at);
CREATE INDEX idx_sessions_app      ON sessions(application_id, started_at);

idle_periods(id PK, started_at TEXT, ended_at TEXT)       -- v2: 空闲单独记录
CREATE INDEX idx_idle_started ON idle_periods(started_at);

settings(key PK, value)      -- idle_timeout_secs 等
meta(key PK, value)          -- schema_version，版本化迁移
```

**不设 daily_stats 物化表的理由**：个人数据量级 ≈ 200 行/天，7 万行/年；带索引的范围 SUM 是微秒级，物化表反而引入「写时聚合不一致 + 跨午夜重算」两类 bug 风险。查询效率真正依赖的是 sessions 上的两个索引。数据积累到多年后再按需加物化或每日 rollup job（schema 版本机制已就位）。时间统一存 **UTC RFC3339 毫秒**（同格式保证字典序=时间序），本地日在查询层换算。

## ⑥ UI 页面结构

设计系统（`src/design` 与 `global.css`）：背景 `#080808`、主卡 `#1c1c1e`、低对比分割线、核心卡圆角 26px / 普通容器 16px、系统字体栈与 tabular-nums；交互动效 150–180ms。深色优先，保留浅色主题。**界面全中文**（含托盘菜单：打开主界面 / 暂停统计 / 退出）。

- **概览** ✅：今天/每周切换 · 总时长大数字 + 上期对比 · 今日同时展示近 7 天趋势和逐时活动 · 周视图展示日均与每日趋势 · 应用分类（已识别标识自动归类，其余为「其他」）· 最常用应用列表 · 暂停提示 · 20s 轮询 + 聚焦刷新
- **应用** ✅：7/30 天切换 · 总时长 · 排行榜（图标/百分比/条形）· 点击展开单应用近 30 天趋势
- **设置** ✅：暂停开关 · 空闲阈值（分钟）· 隐私声明（本地 SQLite，无云端/账号/遥测）
- 侧栏 ✅：概览/应用/设置，Lucide 图标
- ~~Timeline~~：按需求移除（2026-09-23）

应用图标：`platform/icon.rs` 按 app_id 查 `.desktop`（文件名精确匹配 → StartupWMClass 扫描）→ `Icon=` 值在 hicolor/pixmaps 下解析 → base64 送前端；进程内缓存，每应用仅一次文件系统查找；失败回退首字母色块。

- **CPU≈0 的关键是永远阻塞、永不自旋**：Wayland 路径 `next_event` 阻塞在 `poll(socket_fd, timeout)`——无事件时内核挂起线程，空闲时整进程 0% CPU。X11 兜底 1s 轮询仅在做兜底时存在。
- **无 busy loop**：引擎线程 2s 事件粒度（也是 stop 响应上限）+ 10s checkpoint；线程在 `poll`/`sleep` 中度过 99.9% 生命周期。
- **数据库写入**：见 ④，稳态 1 写/10s，空闲 0 写。
- **前端零监控**：不定时检测系统状态；Overview 20s 轮询一次 SQL 聚合，其余页面挂载时拉一次。
- **线程模型**：1 个 tracker 线程 + Tauri 主线程，无线程池。`Mutex<Connection>` 临界区均为微秒级单语句。
- **async 的取舍**：本项目全链路是「等一个文件描述符 + 顺序小事务」，async 化只增加 Tokio 运行时（额外内存/复杂度）毫无收益——**全 std 线程**。command 保持同步 fn（查询微秒级）；若未来出现慢查询再局部 `async fn`（Tauri 会自动移交其 runtime）。
- **内存预期（诚实值）**：Rust 侧引擎+SQLite < 5MB；大头为 WebKitGTK 进程，整应用实测约 60–110MB——这是 Tauri 共享系统 WebView 模型的地板，Svelte 也救不了（见 ②）。若未来要压到 30MB 级，路线是纯原生 UI（egrelg/iced）或常驻无窗口守护进程，属架构级变更，暂不做。
- **启动速度**：无 Tokio、无插件初始化链，冷启动瓶颈仅在 WebView 创建（~300ms 级）。
- **休眠/唤醒与时钟跳变** ✅：会话结束/落盘时刻经单调钟钳制（`capped_now` = min(墙钟 now, 会话开始墙钟 + 实际醒着的 `Instant` 时长 + 6s 宽限)）——挂起与时钟前跳时墙钟差远大于单调时长，会话被压回真实值；时钟回拨由 `ended_at ≥ started_at` 兜底。有单测覆盖（模拟前跳 1 小时被钳到 <60s）。

## ⑧ 第一阶段 MVP：已完成 ✅ + P2 状态

M0 脚手架/托盘/单实例 → M1 平台层（双实现，实测验证）→ M2 引擎状态机 → M3 持久层 → M4 command+UI。
P2 进展（2026-09-23）：

1. ~~P2.1 异常防护~~ ✅ 单调钟钳制 + 单测
2. ~~P2.2 Idle 单独记录~~ ✅ `idle_periods` 表 (schema v2)，空闲段在恢复输入/退出时一次写入（无 UI 展示，数据已留存）
3. ~~P2.3 Overview 增强~~ ✅ 较昨日/上一周期 ↑↓%、今日(逐时)/7天/30天切换、排行百分比
4. ~~P2.4 App 图标~~ ✅ `.desktop` → hicolor 解析 → base64，缓存 + 字母回退
5. ~~P2.5 单应用趋势~~ ✅ 应用页点击展开近 30 天迷你柱状
6. **P2.6 发布工程** ✅：deb 打包（`npm run tauri build`）；开机自启（tauri-plugin-autostart + 设置页开关，Linux 写 `~/.config/autostart`）；浅色/深色主题（token 覆盖 + 防闪烁脚本）；MIT 协议
7. **P2.7 平台扩展**：Windows ✅ 已实现（原生 FFI：`GetForegroundWindow` + `GetWindowTextW` + `QueryFullProcessImageNameW` + `GetLastInputInfo`，1s 轮询共用 `polling.rs` 骨架；经 `RUSTFLAGS='--cfg xcheck' cargo check` 交叉类型检查，待真机验证）；macOS ⬜ 待做（用户要求暂缓）

轮询骨架：`polling.rs` 统一 x11/windows 的去重、空闲抑制、恢复补报与超时语义，平台子类只实现「查窗口」「查空闲毫秒」两个原语（`PollObserver`）。

已移除：Timeline 页面及其 `timeline` 命令/会话明细查询（按需求，2026-09-23）；macOS 实现（用户要求暂缓）。

## ⑨ 每日计划 (Daily Plan, 2026-09-24)

定位：Plan your day → Track your time → Review your day。轻量每日备忘，非项目管理。
刻意不做：标签/项目/子任务/优先级/看板/重复任务/提醒/云同步。

- **Schema v3**：`daily_tasks(id, date 'YYYY-MM-DD', title, completed, estimated_minutes?, note?, created_at, completed_at?)` + `INDEX(date)`；未完成在前（创建序）、已完成在后（完成时间）。`completed` 翻转同步写/清 `completed_at`。
- **命令**：`plan_tasks(date?) / plan_add / plan_toggle / plan_save / plan_delete`（标题去空白非空校验；estimated 范围 1–1440，0/null 即无预估）。
- **UI**：单容器任务列表（`plan-*` 语义类，沿用卡片/圆角/150ms 规范）；圆形勾选（:active 1→1.08 缩放）；完成态灰字+1px 删除线；行内编辑（Enter 保存/Esc 取消）；hover 出 ⋯ 菜单（编辑/删除）；空态「还没有计划」；Enter 连续快速录入；日期 ‹/› 切换可回看/预写任意日期；行聚焦 Delete 删除。
- **与屏幕时间联动**：计划页底部今日小结（计划时长 · 已完成 · 屏幕时间，仅今天）；概览页今日视图顶部「今日计划」小卡（前 4 项 + `x / y 已完成` + 跳转）。
- 性能：无轮询，仅页面挂载读取 + 变更即存（单行写）。「任务 ↔ 应用」自动关联预留于数据结构，未实现。

验证记录：14 个单测全绿（15 连跑无偶发；修复过并行测试临时库路径冲突）、clippy 0 警告、tsc+vite 通过、v1→v2→v3 数据库原地迁移验证、图标解析全命中、Windows 源码交叉类型检查通过（含注入错误的阳性对照）；daily_tasks CRUD/排序/时间戳单测覆盖。
