//! Agent-Skills-style `SKILL.md` checks. Parsing never runs scripts.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path};

use crate::{SkillError, SkillRecord};

const MAX_FILES: usize = 64;
const MAX_DEPTH: usize = 6;
const MAX_FILE_BYTES: u64 = 256 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedSkill {
    pub record: SkillRecord,
    pub body: String,
    pub unknown: Vec<String>,
    pub warnings: Vec<String>,
    pub script_paths: Vec<String>,
}

pub fn inspect_markdown(content: &str) -> Result<ParsedSkill, SkillError> {
    let (front, _) = split_frontmatter(content)?;
    let mut name = None;
    for line in front.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("name:") {
            name = Some(unquote(rest.trim()));
            break;
        }
    }
    let name = name.ok_or_else(|| SkillError::Missing("name".into()))?;
    parse_skill_md(&name, content)
}

pub fn parse_skill_md(dir_name: &str, content: &str) -> Result<ParsedSkill, SkillError> {
    let (front, body) = split_frontmatter(content)?;
    let mut unknown = Vec::new();
    let mut values = Vec::new();
    let mut meta: Vec<(String, String)> = Vec::new();
    let mut in_meta = false;
    for raw in front.lines() {
        if raw.trim().is_empty() {
            continue;
        }
        let indent = raw.len() - raw.trim_start().len();
        let line = raw.trim();
        if in_meta && indent >= 2 {
            let (key, value) = split_key(line)?;
            meta.push((key, unquote(&value)));
            continue;
        }
        in_meta = false;
        let (key, value) = split_key(line)?;
        if key == "metadata" && value.is_empty() {
            in_meta = true;
            continue;
        }
        if !known_top(&key) {
            unknown.push(key.clone());
        }
        values.push((key, unquote(&value)));
    }
    let name = required(&values, "name")?;
    if name != dir_name {
        return Err(SkillError::NameMismatch {
            dir: dir_name.into(),
            name,
        });
    }
    if !valid_name(&name) {
        return Err(SkillError::BadName(name));
    }
    let description = required(&values, "description")?;
    if description.is_empty() {
        return Err(SkillError::Missing("description".into()));
    }
    let version = field(&values, "version")
        .or_else(|| meta_field(&meta, "version"))
        .unwrap_or_default();
    if version.is_empty() {
        return Err(SkillError::Missing("version".into()));
    }
    if !valid_version(&version) {
        return Err(SkillError::BadVersion(version));
    }
    let author = field(&values, "author")
        .or_else(|| meta_field(&meta, "author"))
        .unwrap_or_else(|| "unknown".into());
    let mut warnings = Vec::new();
    for key in &unknown {
        warnings.push(format!("未识别字段 {key}，已保留告警，没有丢弃"));
    }
    Ok(ParsedSkill {
        record: SkillRecord {
            stable_id: name.clone(),
            name,
            display_name: field(&values, "display_name").unwrap_or_default(),
            description,
            version,
            author,
            source: "skill.md".into(),
            tags: split_list(&meta_field(&meta, "domain").unwrap_or_default()),
            boards: split_list(&meta_field(&meta, "boards").unwrap_or_default()),
            required_tools: split_list(&meta_field(&meta, "tools").unwrap_or_default()),
            risk_level: meta_field(&meta, "risk").unwrap_or_else(|| "unknown".into()),
            status: crate::Availability::Available,
        },
        body: body.to_string(),
        unknown,
        warnings,
        script_paths: Vec::new(),
    })
}

pub fn validate_dir(dir: &Path) -> Result<ParsedSkill, SkillError> {
    let name = dir
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| SkillError::BadName(dir.display().to_string()))?;
    let skill_md = dir.join("SKILL.md");
    let content = fs::read_to_string(&skill_md).map_err(|_| SkillError::Missing("SKILL.md".into()))?;
    let mut parsed = parse_skill_md(name, &content)?;
    let mut count = 0usize;
    walk(dir, dir, 0, &mut count, &mut parsed)?;
    let refs = referenced_files(&parsed.body);
    for rel in refs {
        let path = safe_join(dir, &rel)?;
        if !path.is_file() {
            return Err(SkillError::MissingResource(rel));
        }
    }
    Ok(parsed)
}

fn walk(
    root: &Path,
    dir: &Path,
    depth: usize,
    count: &mut usize,
    parsed: &mut ParsedSkill,
) -> Result<(), SkillError> {
    if depth > MAX_DEPTH {
        return Err(SkillError::TooDeep);
    }
    for entry in fs::read_dir(dir).map_err(|e| SkillError::Io(e.to_string()))? {
        let entry = entry.map_err(|e| SkillError::Io(e.to_string()))?;
        *count += 1;
        if *count > MAX_FILES {
            return Err(SkillError::TooManyFiles);
        }
        let path = entry.path();
        let meta = fs::symlink_metadata(&path).map_err(|e| SkillError::Io(e.to_string()))?;
        if meta.file_type().is_symlink() {
            return Err(SkillError::Symlink(path.display().to_string()));
        }
        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        if meta.is_dir() {
            walk(root, &path, depth + 1, count, parsed)?;
        } else if meta.len() > MAX_FILE_BYTES {
            return Err(SkillError::TooLarge);
        } else if rel.starts_with("scripts/") {
            parsed
                .warnings
                .push(format!("{rel} 只作附件保存，安装时不会执行"));
            parsed.script_paths.push(rel);
        }
    }
    Ok(())
}

fn referenced_files(body: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for chunk in body.split("](").skip(1) {
        let Some(end) = chunk.find(')') else { continue };
        let rel = chunk[..end].trim();
        if rel.starts_with("references/") || rel.starts_with("assets/") || rel.starts_with("scripts/")
        {
            out.insert(rel.to_string());
        }
    }
    out
}

pub fn safe_join(root: &Path, rel: &str) -> Result<std::path::PathBuf, SkillError> {
    if rel.starts_with('/') || rel.contains('\\') || rel.contains(':') {
        return Err(SkillError::PathEscape(rel.into()));
    }
    let mut out = root.to_path_buf();
    for part in Path::new(rel).components() {
        match part {
            Component::Normal(p) => out.push(p),
            _ => return Err(SkillError::PathEscape(rel.into())),
        }
    }
    Ok(out)
}

fn split_frontmatter(content: &str) -> Result<(&str, &str), SkillError> {
    let text = content.trim_start();
    if !text.starts_with("---") {
        return Err(SkillError::Missing("frontmatter".into()));
    }
    let rest = text[3..].trim_start_matches(['\r', '\n']);
    let Some(end) = rest.find("\n---") else {
        return Err(SkillError::Missing("frontmatter".into()));
    };
    let front = &rest[..end];
    let body = rest[end + 4..].trim_start_matches(['\r', '\n']);
    Ok((front, body))
}

fn split_key(line: &str) -> Result<(String, String), SkillError> {
    let Some((key, value)) = line.split_once(':') else {
        return Err(SkillError::BadField(line.into()));
    };
    Ok((key.trim().to_string(), value.trim().to_string()))
}

fn unquote(value: &str) -> String {
    value.trim_matches('"').trim().to_string()
}

fn known_top(key: &str) -> bool {
    matches!(
        key,
        "name" | "description" | "license" | "metadata" | "version" | "author" | "display_name"
    )
}

fn required(values: &[(String, String)], key: &str) -> Result<String, SkillError> {
    field(values, key).ok_or_else(|| SkillError::Missing(key.into()))
}

fn field(values: &[(String, String)], key: &str) -> Option<String> {
    values
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.clone())
        .filter(|v| !v.is_empty())
}

fn meta_field(meta: &[(String, String)], key: &str) -> Option<String> {
    field(meta, key)
}

fn split_list(value: &str) -> Vec<String> {
    value
        .split([',', ' '])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

fn valid_name(name: &str) -> bool {
    let mut prev_dash = true;
    if name.len() < 2 || name.len() > 64 {
        return false;
    }
    for c in name.chars() {
        if c == '-' {
            if prev_dash {
                return false;
            }
            prev_dash = true;
        } else if c.is_ascii_lowercase() || c.is_ascii_digit() {
            prev_dash = false;
        } else {
            return false;
        }
    }
    !prev_dash
}

fn valid_version(version: &str) -> bool {
    let mut parts = version.split('.');
    let Some(major) = parts.next() else {
        return false;
    };
    if major.is_empty() || !major.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    parts.all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn good() -> String {
        include_str!("../catalog/esp32-wiring-guide/SKILL.md").to_string()
    }

    #[test]
    fn parses_catalog_skill() {
        let parsed = parse_skill_md("esp32-wiring-guide", &good()).unwrap();
        assert_eq!(parsed.record.version, "1.0.0");
        assert_eq!(parsed.record.author, "livstyle");
        assert!(parsed.record.boards.iter().any(|b| b == "esp32c5"));
    }

    #[test]
    fn rejects_missing_description() {
        let text = "---\nname: esp32-wiring-guide\nmetadata:\n  version: \"1.0.0\"\n---\nbody\n";
        let err = parse_skill_md("esp32-wiring-guide", text).unwrap_err();
        assert!(matches!(err, SkillError::Missing(field) if field == "description"));
    }

    #[test]
    fn rejects_name_mismatch_and_bad_version() {
        let text = "---\nname: other-name\ndescription: x\nmetadata:\n  version: \"1.0.0\"\n---\n";
        assert!(matches!(
            parse_skill_md("esp32-wiring-guide", text),
            Err(SkillError::NameMismatch { .. })
        ));
        let text = "---\nname: esp32-wiring-guide\ndescription: x\nmetadata:\n  version: \"v1\"\n---\n";
        assert!(matches!(
            parse_skill_md("esp32-wiring-guide", text),
            Err(SkillError::BadVersion(_))
        ));
    }

    #[test]
    fn missing_reference_is_an_error_and_scripts_are_not_run() {
        let dir = tempfile::tempdir().unwrap();
        let skill = dir.path().join("demo-skill");
        fs::create_dir_all(skill.join("scripts")).unwrap();
        fs::write(
            skill.join("SKILL.md"),
            "---\nname: demo-skill\ndescription: demo\nmetadata:\n  version: \"1.0.0\"\n---\nsee [s](scripts/run.sh) and [r](references/note.md)\n",
        )
        .unwrap();
        fs::write(skill.join("scripts/run.sh"), "echo no\n").unwrap();
        let err = validate_dir(&skill).unwrap_err();
        assert!(matches!(err, SkillError::MissingResource(_)));
        fs::create_dir_all(skill.join("references")).unwrap();
        fs::write(skill.join("references/note.md"), "note\n").unwrap();
        let parsed = validate_dir(&skill).unwrap();
        assert_eq!(parsed.script_paths, vec!["scripts/run.sh".to_string()]);
        assert!(parsed.warnings.iter().any(|w| w.contains("不会执行")));
    }
}
