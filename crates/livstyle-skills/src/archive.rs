//! ZIP inspection. Entries are checked and, only after that, extracted
//! under a chosen directory. Nothing in the archive is executed.

use std::io::{Cursor, Read};
use std::path::Path;

use zip::ZipArchive;

use crate::parse::safe_join;
use crate::SkillError;

const MAX_ZIP_BYTES: u64 = 2_000_000;
const MAX_ENTRIES: usize = 64;
const MAX_DEPTH: usize = 6;

pub fn inspect_zip(bytes: &[u8]) -> Result<(), SkillError> {
    if bytes.len() as u64 > MAX_ZIP_BYTES {
        return Err(SkillError::TooLarge);
    }
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(|e| SkillError::Zip(e.to_string()))?;
    if archive.len() > MAX_ENTRIES {
        return Err(SkillError::TooManyFiles);
    }
    for index in 0..archive.len() {
        let file = archive
            .by_index(index)
            .map_err(|e| SkillError::Zip(e.to_string()))?;
        check_name(file.name())?;
        if file.unix_mode().is_some_and(|mode| mode & 0o170000 == 0o120000) {
            return Err(SkillError::Symlink(file.name().into()));
        }
    }
    Ok(())
}

pub fn extract_zip(bytes: &[u8], dest: &Path) -> Result<(), SkillError> {
    inspect_zip(bytes)?;
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(|e| SkillError::Zip(e.to_string()))?;
    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|e| SkillError::Zip(e.to_string()))?;
        let name = file.name().to_string();
        if name.ends_with('/') {
            std::fs::create_dir_all(safe_join(dest, name.trim_end_matches('/'))?)
                .map_err(|e| SkillError::Io(e.to_string()))?;
            continue;
        }
        let path = safe_join(dest, &name)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| SkillError::Io(e.to_string()))?;
        }
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)
            .map_err(|e| SkillError::Io(e.to_string()))?;
        std::fs::write(&path, buf).map_err(|e| SkillError::Io(e.to_string()))?;
    }
    Ok(())
}

fn check_name(name: &str) -> Result<(), SkillError> {
    if name.starts_with('/') || name.contains('\\') || name.contains(':') {
        return Err(SkillError::PathEscape(name.into()));
    }
    let mut depth = 0usize;
    for part in name.split('/') {
        if part.is_empty() {
            continue;
        }
        if part == "." || part == ".." {
            return Err(SkillError::PathEscape(name.into()));
        }
        depth += 1;
        if depth > MAX_DEPTH {
            return Err(SkillError::TooDeep);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    fn zip_with(name: &str, bytes: &[u8]) -> Vec<u8> {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .start_file(name, SimpleFileOptions::default())
            .unwrap();
        writer.write_all(bytes).unwrap();
        writer.finish().unwrap().into_inner()
    }

    #[test]
    fn rejects_path_escape_and_does_not_write_outside() {
        let bytes = zip_with("../outside.txt", b"nope");
        let err = inspect_zip(&bytes).unwrap_err();
        assert!(matches!(err, SkillError::PathEscape(_)));
        let dir = tempfile::tempdir().unwrap();
        let outside = dir.path().parent().unwrap().join("outside.txt");
        let _ = std::fs::remove_file(&outside);
        assert!(extract_zip(&bytes, dir.path()).is_err());
        assert!(!outside.exists());
    }
}
