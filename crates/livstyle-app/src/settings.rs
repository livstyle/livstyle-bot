//! Settings and connector actions. Button labels here are the ones the
//! workbench is allowed to show; identical labels collapse to one control.

use std::path::Path;

use serde_json::Value;

/// One label per distinct settings action. The workbench must not add a second
/// control that performs the same action.
pub const SETTINGS_ACTIONS: &[&str] = &[
    "保存 Cursor Key",
    "移除 Cursor Key",
    "重连 Bridge",
    "设为主模型",
    "测试 Judge",
    "探测 WorkBuddy",
    "关闭",
];

pub fn actions_are_unique(labels: &[&str]) -> bool {
    let mut seen = Vec::new();
    for label in labels {
        let text = label.trim();
        if text.is_empty() || seen.contains(&text) {
            return false;
        }
        seen.push(text);
    }
    true
}

/// Which of the three card slots should be drawn. Empty labels and repeated
/// labels stay hidden so a card cannot show two controls for one action.
pub fn shown_card_buttons(labels: [&str; 3]) -> [bool; 3] {
    let mut seen = Vec::new();
    let mut shown = [false; 3];
    for (index, label) in labels.iter().enumerate() {
        let text = label.trim();
        if text.is_empty() || seen.contains(&text) {
            continue;
        }
        seen.push(text);
        shown[index] = true;
    }
    shown
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProductSettings {
    pub llm_base: String,
    pub llm_model: String,
    pub workbuddy_url: String,
}

impl Default for ProductSettings {
    fn default() -> Self {
        Self {
            llm_base: String::new(),
            llm_model: String::new(),
            workbuddy_url: String::new(),
        }
    }
}

pub fn load_product_settings(path: &Path) -> ProductSettings {
    let Ok(text) = std::fs::read_to_string(path) else {
        return ProductSettings::default();
    };
    let Ok(value) = serde_json::from_str::<Value>(&text) else {
        return ProductSettings::default();
    };
    ProductSettings {
        llm_base: value
            .get("llm_base")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        llm_model: value
            .get("llm_model")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        workbuddy_url: value
            .get("workbuddy_url")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
    }
}

pub fn save_product_settings(path: &Path, settings: &ProductSettings) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|err| err.to_string())?;
    }
    let body = serde_json::json!({
        "llm_base": settings.llm_base,
        "llm_model": settings.llm_model,
        "workbuddy_url": settings.workbuddy_url,
    });
    std::fs::write(path, serde_json::to_string_pretty(&body).expect("settings json"))
        .map_err(|err| err.to_string())
}

pub fn llm_family(base_url: &str) -> &'static str {
    if base_url.contains(":11434") {
        "ollama"
    } else {
        "openai"
    }
}

pub fn normalize_llm(base_url: &str, model: &str) -> Result<(String, String, &'static str), String> {
    let base = base_url.trim().trim_end_matches('/').to_string();
    if !(base.starts_with("http://") || base.starts_with("https://")) || base.len() < "http://a".len()
    {
        return Err("模型地址要以 http:// 或 https:// 开头".into());
    }
    if base.chars().any(char::is_whitespace) {
        return Err("模型地址不能有空格".into());
    }
    let model = model.trim().to_string();
    if model.is_empty() || model.chars().any(char::is_whitespace) {
        return Err("要填写一个不带空格的模型名".into());
    }
    let family = llm_family(&base);
    Ok((base, model, family))
}

/// WorkBuddy-style bridge probe stays on the machine. Non-loopback hosts are rejected.
pub fn loopback_http_url(url: &str) -> Result<String, String> {
    let url = url.trim();
    let rest = url
        .strip_prefix("http://")
        .ok_or("WorkBuddy 桥只接受 http:// 开头的本机地址")?;
    if rest.is_empty() {
        return Err("WorkBuddy 桥地址不完整".into());
    }
    let authority = rest.split('/').next().unwrap_or("");
    let host = if let Some(v6) = authority.strip_prefix('[') {
        v6.split(']').next().unwrap_or("")
    } else {
        match authority.rsplit_once(':') {
            Some((host, port)) => {
                if port.is_empty() || !port.chars().all(|c| c.is_ascii_digit()) {
                    return Err("WorkBuddy 桥端口不正确".into());
                }
                host
            }
            None => authority,
        }
    };
    let loopback = host.eq_ignore_ascii_case("localhost") || host == "127.0.0.1" || host == "::1";
    if !loopback {
        return Err("WorkBuddy 桥只接受本机地址，探测不会发到外网".into());
    }
    Ok(url.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_actions_are_unique() {
        assert!(actions_are_unique(SETTINGS_ACTIONS));
        assert_eq!(
            SETTINGS_ACTIONS.iter().filter(|label| **label == "关闭").count(),
            1
        );
    }

    #[test]
    fn repeated_card_labels_collapse_to_one_button() {
        assert_eq!(
            shown_card_buttons(["关闭", "关闭", "关闭"]),
            [true, false, false]
        );
        assert_eq!(shown_card_buttons(["", "", "关闭"]), [false, false, true]);
        assert_eq!(
            shown_card_buttons(["按此执行", "仅补充为上下文", "取消"]),
            [true, true, true]
        );
    }

    #[test]
    fn llm_provider_normalization() {
        let (base, model, family) =
            normalize_llm("http://127.0.0.1:11434/v1/", "qwen2.5-coder:7b").unwrap();
        assert_eq!(base, "http://127.0.0.1:11434/v1");
        assert_eq!(model, "qwen2.5-coder:7b");
        assert_eq!(family, "ollama");
        assert_eq!(
            normalize_llm("https://example.test/v1", "gpt").unwrap().2,
            "openai"
        );
        assert!(normalize_llm("localhost:11434", "qwen").is_err());
        assert!(normalize_llm("http://127.0.0.1:11434/v1", " ").is_err());
    }

    #[test]
    fn workbuddy_probe_stays_on_loopback() {
        assert!(loopback_http_url("http://127.0.0.1:3920/health").is_ok());
        assert!(loopback_http_url("http://localhost:3920").is_ok());
        assert!(loopback_http_url("http://[::1]:3920/").is_ok());
        assert!(loopback_http_url("http://10.0.0.8:3920").is_err());
        assert!(loopback_http_url("https://127.0.0.1:3920").is_err());
    }

    #[test]
    fn product_settings_round_trip() {
        let dir = std::env::temp_dir().join(format!(
            "livstyle-settings-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        let saved = ProductSettings {
            llm_base: "http://127.0.0.1:11434/v1".into(),
            llm_model: "qwen2.5-coder:7b".into(),
            workbuddy_url: "http://127.0.0.1:3920".into(),
        };
        save_product_settings(&path, &saved).unwrap();
        assert_eq!(load_product_settings(&path), saved);
        assert_eq!(
            load_product_settings(&dir.join("missing.json")),
            ProductSettings::default()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
