//! Jev-style System One judge (PRD §8.3, ADR-009).
//!
//! The state is rendered once as a shared prefix; each typed question is a
//! suffix ending in `Answer:` with options labelled `A`, `B`, … . The backend
//! returns next-token log-probabilities, nothing is decoded, and a restricted
//! softmax over the option labels (with an optional fitted temperature) gives
//! the answer distribution. Same technique as jev-rs, implemented natively so
//! livstyle does not need an extra binary.

mod backend;
mod eval;
mod judge;
mod question;

pub use backend::{Backend, BackendError, OllamaRaw, OpenAiCompat, TopLogprobs};
pub use eval::{EvalCase, EvalGold, EvalReport, FieldReport, evaluate, load_cases};
pub use judge::{Calibration, LogprobJudge};
pub use question::{Answer, Question, QuestionKind, score};
