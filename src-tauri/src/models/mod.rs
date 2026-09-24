//! 领域模型。应用时长聚合 / 设置。
//! 序列化友好, 供 command 层直接返回前端。

use serde::{Deserialize, Serialize};

/// 应用时长聚合 (统计查询返回; 应用身份即 app_key, 明细字段并入结果)
#[derive(Debug, Clone, Serialize)]
pub struct AppUsage {
    pub application_id: i64,
    pub app_key: String,
    pub name: String,
    /// 秒
    pub total_secs: f64,
}

/// 运行配置。v1 仅空闲阈值, 存 settings 表 (key-value)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// 输入空闲多久后停止计时 (秒)
    pub idle_timeout_secs: u64,
}

/// 每日计划的一条任务。date 为本地日期 (YYYY-MM-DD), 与时间戳体系无关。
/// 第一阶段只有标题/完成/预估/备注; 「任务 ↔ 应用」关联等留待后续扩展。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanTask {
    pub id: i64,
    pub date: String,
    pub title: String,
    pub completed: bool,
    pub estimated_minutes: Option<i64>,
    pub note: Option<String>,
    pub created_at: String,
    pub completed_at: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            idle_timeout_secs: 60,
        }
    }
}
