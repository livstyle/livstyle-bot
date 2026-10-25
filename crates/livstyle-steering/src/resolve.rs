use serde::{Deserialize, Serialize};

use crate::{Intent, Judgment, Scope};

/// Steering capabilities advertised by the active runtime adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeCaps {
    /// Inject input into a running turn (Octos `turn/steer`).
    pub steer: bool,
    /// Interrupt a running turn (Octos `turn/interrupt`, Cursor run cancel).
    pub interrupt: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ResolvePolicy {
    /// Minimum judgment confidence to apply without escalation. Must come
    /// from calibration on livstyle's own intervention set (ADR-009).
    pub apply_threshold: f32,
    /// Above this probability the input is treated as touching hardware
    /// parameters, which forces the re-approval path. Kept low on purpose.
    pub hardware_flag_threshold: f32,
}

impl Default for ResolvePolicy {
    fn default() -> Self {
        Self {
            apply_threshold: 0.8,
            hardware_flag_threshold: 0.2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Stage {
    /// Fast judge (Jev or rules); low confidence escalates to System Two.
    First,
    /// System Two already answered; low confidence asks the user.
    Final,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    /// Inject into the running turn.
    Steer,
    /// Runtime cannot steer; hold until the next send.
    QueueForNextSend,
    /// Stop the running turn, bump the requirement version, recompute the plan.
    InterruptAndReplan {
        rebuild_expected: bool,
        reapproval_required: bool,
    },
    /// Runtime cannot interrupt: replan applies when the current turn ends.
    ReplanAfterTurn {
        rebuild_expected: bool,
        reapproval_required: bool,
    },
    InterruptStep,
    CancelTask,
    PauseAtBoundary,
    Reprioritize,
    SideAnswer,
    NewTask,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Resolution {
    Apply(Action),
    EscalateToSystemTwo,
    AskUser { candidates: Vec<Intent> },
}

pub fn resolve(
    judgment: &Judgment,
    caps: RuntimeCaps,
    policy: ResolvePolicy,
    stage: Stage,
) -> Resolution {
    if judgment.confidence() < policy.apply_threshold {
        return match stage {
            Stage::First => Resolution::EscalateToSystemTwo,
            Stage::Final => Resolution::AskUser {
                candidates: vec![
                    judgment.intent.value,
                    Intent::AppendContext,
                    Intent::AmendRequirement,
                ]
                .into_iter()
                .fold(Vec::new(), |mut acc, i| {
                    if !acc.contains(&i) {
                        acc.push(i);
                    }
                    acc
                }),
            },
        };
    }

    let reapproval_required = judgment.touches_hardware_params > policy.hardware_flag_threshold;
    let rebuild_expected = judgment.invalidates_artifact > 0.5;

    let action = match judgment.intent.value {
        // Context that turns out to change hardware parameters is a requirement change.
        Intent::AppendContext if reapproval_required => {
            replan(caps, rebuild_expected, reapproval_required)
        }
        Intent::AppendContext if caps.steer => Action::Steer,
        Intent::AppendContext => Action::QueueForNextSend,
        Intent::AmendRequirement => replan(caps, rebuild_expected, reapproval_required),
        Intent::Reprioritize => Action::Reprioritize,
        Intent::Pause => Action::PauseAtBoundary,
        Intent::CancelStep if judgment.scope.value == Scope::WholeTask => Action::CancelTask,
        Intent::CancelStep => Action::InterruptStep,
        Intent::CancelTask => Action::CancelTask,
        Intent::SideQuestion => Action::SideAnswer,
        Intent::NewTask => Action::NewTask,
    };
    Resolution::Apply(action)
}

fn replan(caps: RuntimeCaps, rebuild_expected: bool, reapproval_required: bool) -> Action {
    if caps.interrupt {
        Action::InterruptAndReplan {
            rebuild_expected,
            reapproval_required,
        }
    } else {
        Action::ReplanAfterTurn {
            rebuild_expected,
            reapproval_required,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{JudgeSource, Labeled};

    const FULL: RuntimeCaps = RuntimeCaps {
        steer: true,
        interrupt: true,
    };

    fn judged(intent: Intent, confidence: f32, hw: f32) -> Judgment {
        Judgment {
            intent: Labeled {
                value: intent,
                confidence,
            },
            scope: Labeled {
                value: Scope::RemainingPlan,
                confidence: 0.95,
            },
            touches_hardware_params: hw,
            invalidates_artifact: 0.0,
            urgency: 1.0,
            source: JudgeSource::Rule,
        }
    }

    #[test]
    fn low_confidence_escalates_then_asks_user() {
        let j = judged(Intent::AmendRequirement, 0.5, 0.0);
        let policy = ResolvePolicy::default();
        assert_eq!(
            resolve(&j, FULL, policy, Stage::First),
            Resolution::EscalateToSystemTwo
        );
        assert!(matches!(
            resolve(&j, FULL, policy, Stage::Final),
            Resolution::AskUser { .. }
        ));
    }

    #[test]
    fn append_uses_steer_or_queues() {
        let j = judged(Intent::AppendContext, 0.9, 0.0);
        let policy = ResolvePolicy::default();
        assert_eq!(
            resolve(&j, FULL, policy, Stage::First),
            Resolution::Apply(Action::Steer)
        );
        let no_steer = RuntimeCaps {
            steer: false,
            interrupt: true,
        };
        assert_eq!(
            resolve(&j, no_steer, policy, Stage::First),
            Resolution::Apply(Action::QueueForNextSend)
        );
    }

    #[test]
    fn hardware_context_is_promoted_to_replan_with_reapproval() {
        let j = judged(Intent::AppendContext, 0.9, 0.3);
        assert_eq!(
            resolve(&j, FULL, ResolvePolicy::default(), Stage::First),
            Resolution::Apply(Action::InterruptAndReplan {
                rebuild_expected: false,
                reapproval_required: true,
            })
        );
    }

    #[test]
    fn amend_without_interrupt_waits_for_turn_end() {
        let j = judged(Intent::AmendRequirement, 0.9, 0.0);
        let caps = RuntimeCaps {
            steer: false,
            interrupt: false,
        };
        assert!(matches!(
            resolve(&j, caps, ResolvePolicy::default(), Stage::First),
            Resolution::Apply(Action::ReplanAfterTurn { .. })
        ));
    }
}
