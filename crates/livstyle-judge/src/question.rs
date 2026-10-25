use serde::{Deserialize, Serialize};

use crate::TopLogprobs;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuestionKind {
    /// One of N.
    Choice,
    /// Yes / no.
    Noul,
}

/// A typed question. Option `i` is labelled with letter `A + i`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    pub id: &'static str,
    pub kind: QuestionKind,
    pub text: &'static str,
    /// `(key, description)` pairs.
    pub options: Vec<(&'static str, &'static str)>,
}

impl Question {
    pub fn render(&self) -> String {
        let mut s = format!("Question: {}\n", self.text);
        for (i, (_, desc)) in self.options.iter().enumerate() {
            s.push_str(&format!("{}) {}\n", label(i), desc));
        }
        s
    }

    /// Calibration bucket (question type, option count), as in jev-rs.
    pub fn bucket(&self) -> String {
        format!("{:?}/{}", self.kind, self.options.len()).to_lowercase()
    }
}

fn label(i: usize) -> char {
    (b'A' + i as u8) as char
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Answer {
    pub key: String,
    /// Probability per option key, in option order.
    pub probabilities: Vec<(String, f64)>,
    pub confidence: f64,
    /// Total raw probability mass that landed on option labels; low values
    /// mean the model wanted to say something else.
    pub label_mass: f64,
}

impl Answer {
    pub fn probability(&self, key: &str) -> f64 {
        self.probabilities
            .iter()
            .find(|(k, _)| k == key)
            .map_or(0.0, |(_, p)| *p)
    }
}

/// Restricted softmax over option labels found among the top tokens.
/// Labels missing from the top-k get the smallest observed logprob minus 2
/// so they keep a little mass instead of zero.
pub fn score(question: &Question, top: &TopLogprobs, temperature: f64) -> Option<Answer> {
    let mut found: Vec<Option<f64>> = vec![None; question.options.len()];
    for (token, lp) in top {
        let t = token
            .trim()
            .trim_start_matches(['*', '('])
            .trim_end_matches([')', '.', ':']);
        if t.len() != 1 {
            continue;
        }
        let c = t.chars().next().unwrap().to_ascii_uppercase();
        let idx = (c as usize).wrapping_sub('A' as usize);
        if idx < found.len() {
            let slot = &mut found[idx];
            // Tokens like " B" and "B" can both appear; merge with log-sum-exp.
            *slot = Some(match *slot {
                Some(prev) => log_add(prev, *lp),
                None => *lp,
            });
        }
    }
    if found.iter().all(Option::is_none) {
        return None;
    }
    let floor = top.iter().map(|(_, lp)| *lp).fold(f64::INFINITY, f64::min) - 2.0;
    let label_mass: f64 = found.iter().flatten().map(|lp| lp.exp()).sum();
    let logits: Vec<f64> = found
        .iter()
        .map(|lp| lp.unwrap_or(floor) / temperature)
        .collect();
    let max = logits.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let exps: Vec<f64> = logits.iter().map(|l| (l - max).exp()).collect();
    let sum: f64 = exps.iter().sum();
    let probabilities: Vec<(String, f64)> = question
        .options
        .iter()
        .zip(&exps)
        .map(|((k, _), e)| ((*k).to_owned(), e / sum))
        .collect();
    let (key, confidence) =
        probabilities
            .iter()
            .cloned()
            .fold(
                (String::new(), -1.0),
                |acc, (k, p)| if p > acc.1 { (k, p) } else { acc },
            );
    Some(Answer {
        key,
        probabilities,
        confidence,
        label_mass: label_mass.min(1.0),
    })
}

fn log_add(a: f64, b: f64) -> f64 {
    let m = a.max(b);
    m + ((a - m).exp() + (b - m).exp()).ln()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q() -> Question {
        Question {
            id: "x",
            kind: QuestionKind::Choice,
            text: "t",
            options: vec![("a", "first"), ("b", "second"), ("c", "third")],
        }
    }

    #[test]
    fn restricted_softmax_picks_label() {
        let top = vec![
            (" B".to_owned(), -0.2),
            (" **".to_owned(), -2.0),
            ("A".to_owned(), -3.0),
        ];
        let a = score(&q(), &top, 1.0).unwrap();
        assert_eq!(a.key, "b");
        assert!(a.confidence > 0.9);
        assert!((a.probabilities.iter().map(|(_, p)| p).sum::<f64>() - 1.0).abs() < 1e-9);
        assert!(a.probability("c") > 0.0);
    }

    #[test]
    fn temperature_softens() {
        let top = vec![(" A".to_owned(), -0.1), (" B".to_owned(), -2.5)];
        let sharp = score(&q(), &top, 1.0).unwrap().confidence;
        let soft = score(&q(), &top, 4.0).unwrap().confidence;
        assert!(soft < sharp);
    }

    #[test]
    fn no_labels_is_none() {
        assert!(score(&q(), &vec![("\n".into(), -0.1)], 1.0).is_none());
    }

    #[test]
    fn renders_letters() {
        assert!(q().render().contains("C) third"));
        assert_eq!(q().bucket(), "choice/3");
    }
}
