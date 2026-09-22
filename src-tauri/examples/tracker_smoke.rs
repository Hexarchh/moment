//! 平台事件源冒烟测试: 以真实会话驱动 PlatformTracker, 打印事件流。
//!
//! 运行: cargo run --example tracker_smoke -- [秒数] [空闲阈值秒]
//! 默认运行 30s, 空闲阈值 20s (便于观察 IdleStarted/Resumed)。

use std::time::{Duration, Instant};

use moment_lib::platform::{create_tracker, TrackerEvent};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let run_secs: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let idle_secs: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(20);

    let mut tracker = match create_tracker(Duration::from_secs(idle_secs)) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("FATAL: {e}");
            std::process::exit(1);
        }
    };
    println!(
        "platform={} (运行 {run_secs}s, 空闲阈值 {idle_secs}s, ctrl-c 停止)",
        tracker.name()
    );

    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(run_secs) {
        match tracker.next_event(Duration::from_secs(5)) {
            Ok(Some(TrackerEvent::ActiveWindow(w))) => {
                println!(
                    "[{:>6.1}s] window app='{}' name='{}' title={:?} exe={:?}",
                    start.elapsed().as_secs_f32(),
                    w.app_key,
                    w.name,
                    w.title,
                    w.executable
                );
            }
            Ok(Some(TrackerEvent::IdleStarted)) => {
                println!("[{:>6.1}s] idle started", start.elapsed().as_secs_f32());
            }
            Ok(Some(TrackerEvent::InputResumed)) => {
                println!("[{:>6.1}s] input resumed", start.elapsed().as_secs_f32());
            }
            Ok(None) => {
                println!("[{:>6.1}s] (timeout)", start.elapsed().as_secs_f32());
            }
            Err(e) => {
                eprintln!("tracker error: {e}");
                std::process::exit(2);
            }
        }
    }
    println!("done");
}
