use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Intent of a mid-run user input (PRD §8.3 `intent` question).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Intent {
    AppendContext,
    AmendRequirement,
    Reprioritize,
    Pause,
    CancelStep,
    CancelTask,
    SideQuestion,
    NewTask,
}

impl Intent {
    pub const ALL: [Intent; 8] = [
        Intent::AppendContext,
        Intent::AmendRequirement,
        Intent::Reprioritize,
        Intent::Pause,
        Intent::CancelStep,
        Intent::CancelTask,
        Intent::SideQuestion,
        Intent::NewTask,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    CurrentStep,
    RemainingPlan,
    WholeTask,
    Unrelated,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Labeled<T> {
    pub value: T,
    pub confidence: f32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum JudgeSource {
    Jev { model: String, calibration: String },
    Rule,
    SystemTwo { runtime: String },
    User,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Judgment {
    pub intent: Labeled<Intent>,
    pub scope: Labeled<Scope>,
    /// Probability that the input changes chip, board, flash parameters or partitions.
    pub touches_hardware_params: f32,
    /// Probability that a rebuild is needed.
    pub invalidates_artifact: f32,
    /// 0.0 = can wait for the current step, 2.0 = immediately.
    pub urgency: f32,
    pub source: JudgeSource,
}

impl Judgment {
    pub fn confidence(&self) -> f32 {
        self.intent.confidence.min(self.scope.confidence)
    }
}

/// What the judge sees: the current task state plus the new input.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InterventionContext {
    pub task_summary: String,
    pub current_node: Option<String>,
    pub plan_outline: Vec<String>,
    pub input: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum JudgeError {
    #[error("judge unavailable: {0}")]
    Unavailable(String),
    #[error("judge timed out")]
    Timeout,
    #[error("backend does not expose logprobs")]
    NoLogprobs,
}

pub trait Judge {
    fn judge(&self, ctx: &InterventionContext) -> Result<Judgment, JudgeError>;
}
