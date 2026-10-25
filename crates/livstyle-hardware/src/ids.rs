use std::fmt;

use serde::{Deserialize, Serialize};

/// Milliseconds on the caller-supplied clock. Time is always passed in so the
/// safety logic stays deterministic under test.
pub type Millis = u64;

macro_rules! string_id {
    ($($name:ident),* $(,)?) => {$(
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub String);

        impl $name {
            pub fn new(id: impl Into<String>) -> Self {
                Self(id.into())
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    )*};
}

string_id!(DeviceId, RunnerId, LeaseId, OperationId, ApprovalId);
