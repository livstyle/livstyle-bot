use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Silicon revision as reported by esptool, e.g. `v3.1` (major 3, minor 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ChipRevision {
    pub major: u16,
    pub minor: u16,
}

impl ChipRevision {
    pub const fn new(major: u16, minor: u16) -> Self {
        Self { major, minor }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("invalid chip revision {0:?}, expected `v<major>.<minor>`")]
pub struct RevisionParseError(pub String);

impl FromStr for ChipRevision {
    type Err = RevisionParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || RevisionParseError(s.to_owned());
        let body = s.trim().trim_start_matches(['v', 'V']);
        let (major, minor) = body.split_once('.').ok_or_else(err)?;
        Ok(Self {
            major: major.parse().map_err(|_| err())?,
            minor: minor.parse().map_err(|_| err())?,
        })
    }
}

impl TryFrom<String> for ChipRevision {
    type Error = RevisionParseError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<ChipRevision> for String {
    fn from(value: ChipRevision) -> Self {
        value.to_string()
    }
}

impl fmt::Display for ChipRevision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "v{}.{}", self.major, self.minor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_orders() {
        let a: ChipRevision = "v1.0".parse().unwrap();
        let b: ChipRevision = "3.1".parse().unwrap();
        assert!(a < b);
        assert_eq!(b.to_string(), "v3.1");
        assert!("v3".parse::<ChipRevision>().is_err());
    }
}
