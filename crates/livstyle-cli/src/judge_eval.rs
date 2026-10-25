use std::io::Write;
use std::path::PathBuf;

use clap::{Args, ValueEnum};
use livstyle_judge::{
    Calibration, EvalReport, LogprobJudge, OllamaRaw, OpenAiCompat, evaluate, load_cases,
};
use livstyle_steering::{Judge, RuleJudge};

#[derive(Clone, Copy, ValueEnum)]
pub enum BackendKind {
    Rule,
    Ollama,
    Openai,
}

#[derive(Args)]
pub struct EvalArgs {
    #[arg(long, value_enum, default_value = "ollama")]
    backend: BackendKind,
    /// Ollama root (`ollama`) or OpenAI-compatible `/v1` base (`openai`).
    #[arg(long, default_value = "http://localhost:11434")]
    base: String,
    #[arg(
        long,
        default_value = "hf.co/hesamation/Qwen3.6-35B-A3B-Claude-4.6-Opus-Reasoning-Distilled-GGUF:Q6_K"
    )]
    model: String,
    #[arg(long, default_value = "evals/interventions.jsonl")]
    cases: PathBuf,
    /// Evaluate only the first N cases.
    #[arg(long)]
    limit: Option<usize>,
    /// Confidence needed to auto-apply (PRD §8.4).
    #[arg(long, default_value_t = 0.8)]
    threshold: f64,
    /// Also ask the rebuild question (one more backend call per case).
    #[arg(long)]
    artifact: bool,
    /// Ask the scope question even when the intent implies it.
    #[arg(long)]
    always_scope: bool,
    /// Write the JSON report here.
    #[arg(long)]
    out: Option<PathBuf>,
}

pub fn run(args: &EvalArgs) -> Result<(), String> {
    let mut cases = load_cases(&args.cases)?;
    if let Some(n) = args.limit {
        cases.truncate(n);
    }
    let (name, judge): (String, Box<dyn Judge>) = match args.backend {
        BackendKind::Rule => ("rule".into(), Box::new(RuleJudge)),
        BackendKind::Ollama => {
            let backend = OllamaRaw::new(&args.base, &args.model).map_err(|e| e.to_string())?;
            let mut j = LogprobJudge::new(backend, Calibration::default());
            j.ask_artifact = args.artifact;
            j.always_ask_scope = args.always_scope;
            (format!("ollama:{}", args.model), Box::new(j))
        }
        BackendKind::Openai => {
            let backend = OpenAiCompat::new(&args.base, &args.model).map_err(|e| e.to_string())?;
            let mut j = LogprobJudge::new(backend, Calibration::default());
            j.ask_artifact = args.artifact;
            j.always_ask_scope = args.always_scope;
            (format!("openai-compat:{}", args.model), Box::new(j))
        }
    };
    eprintln!("evaluating {} cases with {name}", cases.len());
    let report = evaluate(
        &name,
        judge.as_ref(),
        &cases,
        args.threshold,
        |i, case, j| {
            let mark = match j {
                Some(j) => format!("{:?} {:.2}", j.intent.value, j.intent.confidence),
                None => "error".into(),
            };
            eprint!(
                "\r[{}/{}] {} -> {mark}            ",
                i + 1,
                cases.len(),
                case.id
            );
            let _ = std::io::stderr().flush();
        },
    );
    eprintln!();
    print_report(&report);
    if let Some(out) = &args.out {
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(out, serde_json::to_string_pretty(&report).unwrap())
            .map_err(|e| e.to_string())?;
        println!("report written to {}", out.display());
    }
    Ok(())
}

fn print_report(r: &EvalReport) {
    println!("judge: {}", r.judge);
    println!("cases: {} (errors {})", r.cases, r.failures);
    for (label, f) in [("intent", &r.intent), ("scope", &r.scope)] {
        println!(
            "{label:<7} accuracy {:.1}%  ECE {:.3}  coverage@{:.2} {:.1}%  accuracy@{:.2} {:.1}%",
            f.accuracy * 100.0,
            f.ece,
            r.threshold,
            f.coverage_at_threshold * 100.0,
            r.threshold,
            f.accuracy_at_threshold * 100.0
        );
    }
    println!(
        "hardware-param flag accuracy {:.1}%, false negatives {}",
        r.hardware_accuracy * 100.0,
        r.hardware_false_negatives
    );
    println!(
        "latency per input p50 {} ms, p95 {} ms",
        r.latency_p50_ms, r.latency_p95_ms
    );
    if !r.misses.is_empty() {
        println!("intent misses ({}):", r.misses.len());
        for m in r.misses.iter().take(30) {
            println!("  {m}");
        }
    }
}
