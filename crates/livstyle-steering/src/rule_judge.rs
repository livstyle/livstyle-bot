use crate::{
    Intent, InterventionContext, Judge, JudgeError, JudgeSource, Judgment, Labeled, Scope,
};

/// Keyword fallback used when Jev is unavailable (PRD §8.3). Deliberately
/// conservative: anything it cannot place confidently escalates.
#[derive(Debug, Default, Clone)]
pub struct RuleJudge;

const CANCEL_TASK: &[&str] = &[
    "取消任务",
    "不做了",
    "全部停止",
    "cancel task",
    "abort task",
    "stop everything",
];
const CANCEL_STEP: &[&str] = &["不用做了", "跳过", "skip this", "cancel this step"];
const PAUSE: &[&str] = &["暂停", "等一下", "先停", "pause", "hold on", "wait"];
const NEW_TASK: &[&str] = &["另外帮我", "新任务", "另开", "new task", "separately"];
const AMEND: &[&str] = &[
    "改成",
    "换成",
    "改为",
    "不要",
    "而是",
    "instead",
    "change to",
    "switch to",
];
const QUESTION: &[&str] = &[
    "?",
    "？",
    "是什么",
    "哪个",
    "为什么",
    "what",
    "which",
    "why",
];
const REPRIORITIZE: &[&str] = &["先别", "先只", "优先", "only build", "first"];
const HARDWARE: &[&str] = &[
    "芯片",
    "板子",
    "开发板",
    "分区",
    "烧录",
    "波特率",
    "flash",
    "partition",
    "baud",
    "board",
    "esp32",
    "idf_target",
    "sdkconfig",
    "psram",
];
const REBUILD: &[&str] = &[
    "sdkconfig",
    "menuconfig",
    "idf_target",
    "芯片",
    "分区",
    "partition",
    "代码",
    "code",
];

fn hit(text: &str, words: &[&str]) -> bool {
    words.iter().any(|w| text.contains(w))
}

impl Judge for RuleJudge {
    fn judge(&self, ctx: &InterventionContext) -> Result<Judgment, JudgeError> {
        let text = ctx.input.to_lowercase();
        let (intent, confidence, scope) = if hit(&text, CANCEL_TASK) {
            (Intent::CancelTask, 0.9, Scope::WholeTask)
        } else if hit(&text, NEW_TASK) {
            (Intent::NewTask, 0.85, Scope::Unrelated)
        } else if hit(&text, PAUSE) {
            (Intent::Pause, 0.85, Scope::CurrentStep)
        } else if hit(&text, CANCEL_STEP) {
            (Intent::CancelStep, 0.85, Scope::CurrentStep)
        } else if hit(&text, REPRIORITIZE) {
            (Intent::Reprioritize, 0.6, Scope::RemainingPlan)
        } else if hit(&text, AMEND) {
            (Intent::AmendRequirement, 0.6, Scope::RemainingPlan)
        } else if hit(&text, QUESTION) {
            (Intent::SideQuestion, 0.6, Scope::Unrelated)
        } else {
            (Intent::AppendContext, 0.4, Scope::CurrentStep)
        };

        Ok(Judgment {
            intent: Labeled {
                value: intent,
                confidence,
            },
            scope: Labeled {
                value: scope,
                confidence,
            },
            touches_hardware_params: if hit(&text, HARDWARE) { 0.9 } else { 0.1 },
            invalidates_artifact: if hit(&text, REBUILD) { 0.8 } else { 0.2 },
            urgency: if intent == Intent::CancelTask {
                2.0
            } else {
                1.0
            },
            source: JudgeSource::Rule,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn judge(input: &str) -> Judgment {
        RuleJudge
            .judge(&InterventionContext {
                input: input.into(),
                ..Default::default()
            })
            .unwrap()
    }

    #[test]
    fn clear_commands_are_confident() {
        assert_eq!(judge("不做了").intent.value, Intent::CancelTask);
        assert_eq!(judge("等一下").intent.value, Intent::Pause);
        assert!(judge("不做了").confidence() >= 0.8);
    }

    #[test]
    fn ambiguous_input_is_not_confident() {
        let j = judge("板子是 8MB Flash 的版本");
        assert_eq!(j.intent.value, Intent::AppendContext);
        assert!(j.confidence() < 0.8);
        assert!(j.touches_hardware_params > 0.5);
    }

    #[test]
    fn chip_switch_is_amend_with_rebuild() {
        let j = judge("先别烧录，换成 C6 板子");
        assert!(matches!(
            j.intent.value,
            Intent::Reprioritize | Intent::AmendRequirement
        ));
        assert!(j.touches_hardware_params > 0.5);
    }
}
