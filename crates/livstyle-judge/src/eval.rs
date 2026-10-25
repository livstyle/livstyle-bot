use std::path::Path;
use std::time::{Duration, Instant};

use livstyle_steering::{InterventionContext, Judge, Judgment};
use serde::{Deserialize, Serialize};

use crate::judge::{intent_key, scope_key};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvalGold {
    pub intent: String,
    pub scope: String,
    pub touches_hardware_params: bool,
}

/// One line of `evals/interventions.jsonl`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvalCase {
    pub id: String,
    pub task: String,
    #[serde(default)]
    pub step: Option<String>,
    pub input: String,
    pub gold: EvalGold,
}

impl EvalCase {
    pub fn context(&self) -> InterventionContext {
        InterventionContext {
            task_summary: self.task.clone(),
            current_node: self.step.clone(),
            plan_outline: Vec::new(),
            input: self.input.clone(),
        }
    }
}

pub fn load_cases(path: &Path) -> Result<Vec<EvalCase>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    text.lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty() && !l.trim_start().starts_with("//"))
        .map(|(i, l)| serde_json::from_str(l).map_err(|e| format!("line {}: {e}", i + 1)))
        .collect()
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FieldReport {
    pub accuracy: f64,
    /// Top-label expected calibration error over 10 equal-width bins.
    pub ece: f64,
    /// Fraction of cases at or above the threshold, and accuracy among them.
    pub coverage_at_threshold: f64,
    pub accuracy_at_threshold: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EvalReport {
    pub judge: String,
    pub cases: usize,
    pub failures: usize,
    pub threshold: f64,
    pub intent: FieldReport,
    pub scope: FieldReport,
    pub hardware_accuracy: f64,
    /// Gold says hardware parameters change but the judge said no (at 0.2):
    /// the safety-relevant error.
    pub hardware_false_negatives: usize,
    pub latency_p50_ms: u64,
    pub latency_p95_ms: u64,
    pub misses: Vec<String>,
}

struct Row {
    intent_ok: bool,
    intent_conf: f64,
    scope_ok: bool,
    scope_conf: f64,
    hw_ok: bool,
    hw_false_negative: bool,
}

fn field(rows: &[(bool, f64)], threshold: f64) -> FieldReport {
    let n = rows.len().max(1) as f64;
    let accuracy = rows.iter().filter(|(ok, _)| *ok).count() as f64 / n;
    let mut ece = 0.0;
    for b in 0..10 {
        let (lo, hi) = (b as f64 / 10.0, (b + 1) as f64 / 10.0);
        let bin: Vec<_> = rows
            .iter()
            .filter(|(_, c)| *c >= lo && (*c < hi || (b == 9 && *c <= 1.0)))
            .collect();
        if bin.is_empty() {
            continue;
        }
        let acc = bin.iter().filter(|(ok, _)| *ok).count() as f64 / bin.len() as f64;
        let conf = bin.iter().map(|(_, c)| c).sum::<f64>() / bin.len() as f64;
        ece += (bin.len() as f64 / n) * (acc - conf).abs();
    }
    let confident: Vec<_> = rows.iter().filter(|(_, c)| *c >= threshold).collect();
    FieldReport {
        accuracy,
        ece,
        coverage_at_threshold: confident.len() as f64 / n,
        accuracy_at_threshold: if confident.is_empty() {
            0.0
        } else {
            confident.iter().filter(|(ok, _)| *ok).count() as f64 / confident.len() as f64
        },
    }
}

fn percentile(sorted: &[Duration], p: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let idx = ((sorted.len() - 1) as f64 * p).round() as usize;
    sorted[idx].as_millis() as u64
}

/// Runs every case through `judge`. `progress` is called after each case.
pub fn evaluate(
    name: &str,
    judge: &dyn Judge,
    cases: &[EvalCase],
    threshold: f64,
    mut progress: impl FnMut(usize, &EvalCase, Option<&Judgment>),
) -> EvalReport {
    let mut rows = Vec::new();
    let mut latencies = Vec::new();
    let mut failures = 0;
    let mut misses = Vec::new();
    for (i, case) in cases.iter().enumerate() {
        let start = Instant::now();
        let result = judge.judge(&case.context());
        latencies.push(start.elapsed());
        match &result {
            Ok(j) => {
                let intent = intent_key(j.intent.value);
                let scope = scope_key(j.scope.value);
                let hw = j.touches_hardware_params > 0.2;
                let row = Row {
                    intent_ok: intent == case.gold.intent,
                    intent_conf: j.intent.confidence as f64,
                    scope_ok: scope == case.gold.scope,
                    scope_conf: j.scope.confidence as f64,
                    hw_ok: hw == case.gold.touches_hardware_params,
                    hw_false_negative: case.gold.touches_hardware_params && !hw,
                };
                if !row.intent_ok {
                    misses.push(format!(
                        "{}: intent {} (gold {}) conf {:.2}",
                        case.id, intent, case.gold.intent, j.intent.confidence
                    ));
                }
                rows.push(row);
            }
            Err(e) => {
                failures += 1;
                misses.push(format!("{}: error {e}", case.id));
            }
        }
        progress(i, case, result.as_ref().ok());
    }
    latencies.sort();
    let n = rows.len().max(1) as f64;
    EvalReport {
        judge: name.to_owned(),
        cases: cases.len(),
        failures,
        threshold,
        intent: field(
            &rows
                .iter()
                .map(|r| (r.intent_ok, r.intent_conf))
                .collect::<Vec<_>>(),
            threshold,
        ),
        scope: field(
            &rows
                .iter()
                .map(|r| (r.scope_ok, r.scope_conf))
                .collect::<Vec<_>>(),
            threshold,
        ),
        hardware_accuracy: rows.iter().filter(|r| r.hw_ok).count() as f64 / n,
        hardware_false_negatives: rows.iter().filter(|r| r.hw_false_negative).count(),
        latency_p50_ms: percentile(&latencies, 0.5),
        latency_p95_ms: percentile(&latencies, 0.95),
        misses,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use livstyle_steering::RuleJudge;

    #[test]
    fn bundled_dataset_loads_and_rule_judge_runs() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../evals/interventions.jsonl");
        let cases = load_cases(&path).unwrap();
        assert!(cases.len() >= 100, "Spike 4 needs at least 100 cases");
        let intents: std::collections::BTreeSet<_> =
            cases.iter().map(|c| c.gold.intent.as_str()).collect();
        assert_eq!(intents.len(), 8, "every intent must be covered");
        let report = evaluate("rule", &RuleJudge, &cases, 0.8, |_, _, _| {});
        assert_eq!(report.failures, 0);
        assert!(report.intent.accuracy > 0.0);
    }

    #[test]
    fn ece_of_perfectly_calibrated_bins_is_zero() {
        let rows = vec![(true, 1.0), (true, 1.0), (false, 0.0)];
        let f = field(&rows, 0.8);
        assert!(f.ece.abs() < 1e-9);
        assert!((f.coverage_at_threshold - 2.0 / 3.0).abs() < 1e-9);
        assert_eq!(f.accuracy_at_threshold, 1.0);
    }
}
