//! Product-level domain model owned by the livstyle Product Service (PRD §5.2, §6, §7).

mod ids;
mod iteration;
mod levels;
mod plan;
mod task;

pub use ids::TaskId;
pub use iteration::{BoardRuntime, ChangeKind, IterationPlan, IterationStep, plan_iteration};
pub use levels::{RiskLevel, RiskPolicy, VerificationLevel};
pub use plan::{
    Interruptibility, NodeId, NodeKind, NodeStatus, Plan, PlanDiff, PlanError, PlanNode,
};
pub use task::{ProductTask, TaskError, TaskStatus};
