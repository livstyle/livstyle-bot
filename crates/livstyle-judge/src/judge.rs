use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use livstyle_steering::{
    Intent, InterventionContext, Judge, JudgeError, JudgeSource, Judgment, Labeled, Scope,
};
use serde::{Deserialize, Serialize};

use crate::{Answer, Backend, BackendError, Question, QuestionKind, score};

const SYSTEM: &str = "You are the System One judge of an embedded-firmware development agent. \
A user typed a message while a task was running. Classify the message. \
Answer every question with exactly one option letter.";

/// Per-bucket temperatures fitted offline (`bucket -> T`). Missing buckets use 1.0.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Calibration {
    pub name: String,
    #[serde(default)]
    pub temperatures: BTreeMap<String, f64>,
}

impl Calibration {
    fn temperature(&self, q: &Question) -> f64 {
        self.temperatures.get(&q.bucket()).copied().unwrap_or(1.0)
    }
}

pub(crate) fn intent_question() -> Question {
    Question {
        id: "intent",
        kind: QuestionKind::Choice,
        text: "What does the user want?",
        options: vec![
            (
                "append_context",
                "Add information or a detail for the current work, without changing the goal",
            ),
            (
                "amend_requirement",
                "Change the requirement, target, chip/board or expected behaviour",
            ),
            (
                "reprioritize",
                "Change the order or skip/limit some remaining steps, keeping the goal",
            ),
            ("pause", "Pause or wait, to continue later"),
            ("cancel_step", "Stop or skip only the current step"),
            ("cancel_task", "Stop the whole task"),
            (
                "side_question",
                "Ask a question that needs an answer but does not change the task",
            ),
            ("new_task", "Start a separate, unrelated task"),
        ],
    }
}

pub(crate) fn scope_question() -> Question {
    Question {
        id: "scope",
        kind: QuestionKind::Choice,
        text: "Which part of the task does the message affect?",
        options: vec![
            ("current_step", "Only the step that is running now"),
            (
                "remaining_plan",
                "Steps that have not run yet, or results that must be redone",
            ),
            ("whole_task", "The entire task"),
            ("unrelated", "Nothing in this task"),
        ],
    }
}

pub(crate) fn hardware_question() -> Question {
    Question {
        id: "touches_hardware_params",
        kind: QuestionKind::Noul,
        text: "Does the message change the target chip, board, flash/partition settings or other parameters of writing to the device?",
        options: vec![("yes", "Yes"), ("no", "No")],
    }
}

pub(crate) fn artifact_question() -> Question {
    Question {
        id: "invalidates_artifact",
        kind: QuestionKind::Noul,
        text: "Would the firmware need to be rebuilt because of this message?",
        options: vec![("yes", "Yes"), ("no", "No")],
    }
}

/// Answers for one judgment, kept for evaluation and audit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Trace {
    pub answers: BTreeMap<String, Answer>,
    pub latency: Duration,
}

/// Scope implied by the intent itself. Only `append_context` and
/// `amend_requirement` can touch either the running step or later steps, so
/// only they need the scope question.
fn implied_scope(intent: Intent) -> Option<Scope> {
    match intent {
        Intent::CancelStep => Some(Scope::CurrentStep),
        Intent::Reprioritize => Some(Scope::RemainingPlan),
        Intent::Pause | Intent::CancelTask => Some(Scope::WholeTask),
        Intent::SideQuestion | Intent::NewTask => Some(Scope::Unrelated),
        Intent::AppendContext | Intent::AmendRequirement => None,
    }
}

pub struct LogprobJudge<B: Backend> {
    backend: B,
    calibration: Calibration,
    /// Ask the rebuild question. Off by default: the plan graph's input
    /// digests decide rebuilds deterministically, so this only costs latency.
    pub ask_artifact: bool,
    /// Ask the scope question even when the intent implies the scope.
    pub always_ask_scope: bool,
}

impl<B: Backend> LogprobJudge<B> {
    pub fn new(backend: B, calibration: Calibration) -> Self {
        Self {
            backend,
            calibration,
            ask_artifact: false,
            always_ask_scope: false,
        }
    }

    pub fn backend(&self) -> &B {
        &self.backend
    }

    fn state(ctx: &InterventionContext) -> String {
        let mut s = format!("Task: {}\n", ctx.task_summary);
        if let Some(node) = &ctx.current_node {
            s.push_str(&format!("Running step: {node}\n"));
        }
        if !ctx.plan_outline.is_empty() {
            s.push_str("Plan:\n");
            for step in &ctx.plan_outline {
                s.push_str(&format!("- {step}\n"));
            }
        }
        s.push_str(&format!("New user message: \"{}\"\n\n", ctx.input));
        s
    }

    fn ask(&self, state: &str, q: &Question) -> Result<Answer, BackendError> {
        let top = self
            .backend
            .top_logprobs(SYSTEM, &format!("{state}{}", q.render()))?;
        score(q, &top, self.calibration.temperature(q)).ok_or(BackendError::NoLogprobs)
    }

    pub fn judge_traced(&self, ctx: &InterventionContext) -> Result<(Judgment, Trace), JudgeError> {
        let start = Instant::now();
        let state = Self::state(ctx);
        let map_err = |e: BackendError| match e {
            BackendError::NoLogprobs => JudgeError::NoLogprobs,
            other => JudgeError::Unavailable(other.to_string()),
        };
        let mut answers = BTreeMap::new();
        let intent_a = self.ask(&state, &intent_question()).map_err(map_err)?;
        let intent = parse_intent(&intent_a.key);
        let intent_conf = intent_a.confidence as f32;
        answers.insert("intent".to_owned(), intent_a);

        let mut questions = vec![hardware_question()];
        let implied = implied_scope(intent).filter(|_| !self.always_ask_scope);
        if implied.is_none() {
            questions.push(scope_question());
        }
        if self.ask_artifact {
            questions.push(artifact_question());
        }
        for q in &questions {
            answers.insert(q.id.to_owned(), self.ask(&state, q).map_err(map_err)?);
        }

        let scope = match (implied, answers.get("scope")) {
            (Some(s), _) => Labeled {
                value: s,
                confidence: intent_conf,
            },
            (None, Some(a)) => Labeled {
                value: parse_scope(&a.key),
                confidence: a.confidence as f32,
            },
            (None, None) => unreachable!("scope question asked when not implied"),
        };
        let judgment = Judgment {
            intent: Labeled {
                value: intent,
                confidence: intent_conf,
            },
            scope,
            touches_hardware_params: answers["touches_hardware_params"].probability("yes") as f32,
            invalidates_artifact: answers
                .get("invalidates_artifact")
                .map_or(0.5, |a| a.probability("yes")) as f32,
            urgency: 1.0,
            source: JudgeSource::Jev {
                model: self.backend.model_id(),
                calibration: self.calibration.name.clone(),
            },
        };
        Ok((
            judgment,
            Trace {
                answers,
                latency: start.elapsed(),
            },
        ))
    }
}

impl<B: Backend> Judge for LogprobJudge<B> {
    fn judge(&self, ctx: &InterventionContext) -> Result<Judgment, JudgeError> {
        self.judge_traced(ctx).map(|(j, _)| j)
    }
}

pub(crate) fn parse_intent(key: &str) -> Intent {
    Intent::ALL
        .into_iter()
        .find(|i| intent_key(*i) == key)
        .unwrap_or(Intent::AppendContext)
}

pub(crate) fn intent_key(i: Intent) -> &'static str {
    match i {
        Intent::AppendContext => "append_context",
        Intent::AmendRequirement => "amend_requirement",
        Intent::Reprioritize => "reprioritize",
        Intent::Pause => "pause",
        Intent::CancelStep => "cancel_step",
        Intent::CancelTask => "cancel_task",
        Intent::SideQuestion => "side_question",
        Intent::NewTask => "new_task",
    }
}

pub(crate) fn parse_scope(key: &str) -> Scope {
    match key {
        "current_step" => Scope::CurrentStep,
        "remaining_plan" => Scope::RemainingPlan,
        "whole_task" => Scope::WholeTask,
        _ => Scope::Unrelated,
    }
}

pub(crate) fn scope_key(s: Scope) -> &'static str {
    match s {
        Scope::CurrentStep => "current_step",
        Scope::RemainingPlan => "remaining_plan",
        Scope::WholeTask => "whole_task",
        Scope::Unrelated => "unrelated",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TopLogprobs;

    /// Answers by keyword in the rendered question, to test wiring without a model.
    struct Scripted {
        intent: &'static str,
    }

    impl Backend for Scripted {
        fn model_id(&self) -> String {
            "scripted".into()
        }

        fn top_logprobs(&self, _system: &str, user: &str) -> Result<TopLogprobs, BackendError> {
            let letter = if user.contains("What does the user want?") {
                self.intent
            } else if user.contains("Which part") {
                " B"
            } else {
                " A"
            };
            Ok(vec![(letter.into(), -0.05), (" D".into(), -3.5)])
        }
    }

    fn ctx(input: &str) -> InterventionContext {
        InterventionContext {
            task_summary: "fix boot".into(),
            input: input.into(),
            ..Default::default()
        }
    }

    #[test]
    fn implied_scope_skips_the_scope_question() {
        let judge = LogprobJudge::new(Scripted { intent: " F" }, Calibration::default());
        let (j, trace) = judge.judge_traced(&ctx("不做了")).unwrap();
        assert_eq!(j.intent.value, Intent::CancelTask);
        assert_eq!(j.scope.value, Scope::WholeTask);
        assert_eq!(j.scope.confidence, j.intent.confidence);
        assert!(j.touches_hardware_params > 0.9);
        assert!(j.confidence() > 0.9);
        let asked: Vec<_> = trace.answers.keys().map(String::as_str).collect();
        assert_eq!(asked, ["intent", "touches_hardware_params"]);
        assert!(matches!(j.source, JudgeSource::Jev { .. }));
    }

    #[test]
    fn amend_asks_scope_and_optional_artifact() {
        let mut judge = LogprobJudge::new(Scripted { intent: " B" }, Calibration::default());
        judge.ask_artifact = true;
        let (j, trace) = judge.judge_traced(&ctx("改成 C6")).unwrap();
        assert_eq!(j.intent.value, Intent::AmendRequirement);
        assert_eq!(j.scope.value, Scope::RemainingPlan);
        assert_eq!(trace.answers.len(), 4);
        assert!(j.invalidates_artifact > 0.9);
    }

    #[test]
    fn keys_round_trip() {
        for i in Intent::ALL {
            assert_eq!(parse_intent(intent_key(i)), i);
        }
        assert_eq!(intent_question().options.len(), Intent::ALL.len());
    }
}
