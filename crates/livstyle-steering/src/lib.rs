//! Live steering (PRD §8): every user input during a run is persisted, judged
//! and resolved into an explicit action. A judge only advises; it never
//! authorizes hardware operations.

mod judge;
mod queue;
mod resolve;
mod rule_judge;

pub use judge::{
    Intent, InterventionContext, Judge, JudgeError, JudgeSource, Judgment, Labeled, Scope,
};
pub use queue::{InterventionId, InterventionQueue, InterventionRecord, InterventionStatus};
pub use resolve::{Action, Resolution, ResolvePolicy, RuntimeCaps, Stage, resolve};
pub use rule_judge::RuleJudge;
