//! 迁移子模块：planner（计划+锁定检测）、copier（方案P）、locker/killer（占用关闭）。

pub mod copier;
pub mod killer;
pub mod locker;
pub mod planner;

pub use copier::execute_plan;
pub use killer::kill_locks;
pub use locker::scan_locks;
pub use planner::build_plan;
