use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::{BoardProfile, Capability, ChipProfile, SupportTier};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Error)]
pub enum ProfileError {
    #[error("read {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("parse {path}: {source}")]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error("{path}: unsupported schema_version {found}, expected {SCHEMA_VERSION}")]
    Schema { path: PathBuf, found: u32 },
    #[error("duplicate {kind} profile {id:?}")]
    Duplicate { kind: &'static str, id: String },
    #[error("board {board:?} references unknown chip {chip:?}")]
    UnknownChip { board: String, chip: String },
    #[error("board {board:?} reserves gpio {gpio} more than once")]
    DuplicatePin { board: String, gpio: u8 },
    #[error("chip {chip:?}: {reason}")]
    InvalidChip { chip: String, reason: String },
}

/// Loaded and cross-validated set of chip and board profiles.
#[derive(Debug, Clone, Default)]
pub struct Registry {
    chips: BTreeMap<String, ChipProfile>,
    boards: BTreeMap<String, BoardProfile>,
}

impl Registry {
    /// Loads `<root>/chips/*.toml` and `<root>/boards/*.toml`.
    pub fn load_dir(root: impl AsRef<Path>) -> Result<Self, ProfileError> {
        let root = root.as_ref();
        let mut registry = Self::default();
        for path in toml_files(&root.join("chips"))? {
            let chip: ChipProfile = parse(&path)?;
            check_schema(&path, chip.schema_version)?;
            registry.insert_chip(chip)?;
        }
        for path in toml_files(&root.join("boards"))? {
            let board: BoardProfile = parse(&path)?;
            check_schema(&path, board.schema_version)?;
            registry.insert_board(board)?;
        }
        registry.validate()?;
        Ok(registry)
    }

    pub fn insert_chip(&mut self, chip: ChipProfile) -> Result<(), ProfileError> {
        if chip.hp_cores == 0 {
            return Err(ProfileError::InvalidChip {
                chip: chip.chip,
                reason: "hp_cores must be at least 1".into(),
            });
        }
        if let (Some(min), Some(max)) = (chip.revisions.min, chip.revisions.max) {
            if min > max {
                return Err(ProfileError::InvalidChip {
                    chip: chip.chip,
                    reason: format!("revision min {min} > max {max}"),
                });
            }
        }
        if self.chips.contains_key(&chip.chip) {
            return Err(ProfileError::Duplicate {
                kind: "chip",
                id: chip.chip,
            });
        }
        self.chips.insert(chip.chip.clone(), chip);
        Ok(())
    }

    pub fn insert_board(&mut self, board: BoardProfile) -> Result<(), ProfileError> {
        if self.boards.contains_key(&board.id) {
            return Err(ProfileError::Duplicate {
                kind: "board",
                id: board.id,
            });
        }
        let mut pins = HashSet::new();
        for pin in &board.reserved_pins {
            if !pins.insert(pin.gpio) {
                return Err(ProfileError::DuplicatePin {
                    board: board.id.clone(),
                    gpio: pin.gpio,
                });
            }
        }
        self.boards.insert(board.id.clone(), board);
        Ok(())
    }

    pub fn validate(&self) -> Result<(), ProfileError> {
        for board in self.boards.values() {
            let referenced =
                std::iter::once(&board.chip).chain(board.companion.as_ref().map(|c| &c.chip));
            for chip in referenced {
                if !self.chips.contains_key(chip) {
                    return Err(ProfileError::UnknownChip {
                        board: board.id.clone(),
                        chip: chip.clone(),
                    });
                }
            }
        }
        Ok(())
    }

    pub fn chip(&self, chip: &str) -> Option<&ChipProfile> {
        self.chips.get(chip)
    }

    pub fn board(&self, id: &str) -> Option<&BoardProfile> {
        self.boards.get(id)
    }

    pub fn chips(&self) -> impl Iterator<Item = &ChipProfile> {
        self.chips.values()
    }

    pub fn boards(&self) -> impl Iterator<Item = &BoardProfile> {
        self.boards.values()
    }

    /// A board is only as supported as its chip.
    pub fn effective_tier(&self, board_id: &str) -> SupportTier {
        let Some(board) = self.boards.get(board_id) else {
            return SupportTier::Unsupported;
        };
        let chip_tier = self
            .chips
            .get(&board.chip)
            .map_or(SupportTier::Unsupported, |c| c.support_tier);
        board.support_tier.min(chip_tier)
    }

    /// Whether the board, including its companion chip, provides `capability`.
    /// Returns the chip that provides it.
    pub fn provider_of(&self, board_id: &str, capability: Capability) -> Option<&ChipProfile> {
        let board = self.boards.get(board_id)?;
        let main = self.chips.get(&board.chip)?;
        if main.has(capability) {
            return Some(main);
        }
        let companion = self.chips.get(&board.companion.as_ref()?.chip)?;
        companion.has(capability).then_some(companion)
    }
}

fn toml_files(dir: &Path) -> Result<Vec<PathBuf>, ProfileError> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let io = |source| ProfileError::Io {
        path: dir.to_owned(),
        source,
    };
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(io)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|ext| ext == "toml"))
        .collect();
    files.sort();
    Ok(files)
}

fn parse<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, ProfileError> {
    let text = fs::read_to_string(path).map_err(|source| ProfileError::Io {
        path: path.to_owned(),
        source,
    })?;
    toml::from_str(&text).map_err(|source| ProfileError::Parse {
        path: path.to_owned(),
        source,
    })
}

fn check_schema(path: &Path, found: u32) -> Result<(), ProfileError> {
    if found != SCHEMA_VERSION {
        return Err(ProfileError::Schema {
            path: path.to_owned(),
            found,
        });
    }
    Ok(())
}
