use livstyle_secrets::{CURSOR_API_KEY, CredentialStore, ensure_env_file};

fn store() -> Result<CredentialStore, String> {
    CredentialStore::default_location().map_err(|e| e.to_string())
}

/// Points at the env file. The key is written by the user, not typed into this process.
pub fn login() -> Result<(), String> {
    let path = ensure_env_file().map_err(|e| e.to_string())?;
    println!("把 Cursor API Key 写在这个文件里：");
    println!("  {}", path.display());
    println!();
    println!("CURSOR_API_KEY=");
    println!();
    println!("写在等号后面并保存。不要把这个文件提交到仓库。");
    println!("写好后执行：livstyle auth status");
    Ok(())
}

pub fn status() -> Result<(), String> {
    let store = store()?;
    match store.get_hit(CURSOR_API_KEY).map_err(|e| e.to_string())? {
        Some(hit) => println!(
            "{CURSOR_API_KEY}: 已配置，指纹 {}（{}）",
            hit.secret.fingerprint(),
            hit.source
        ),
        None => {
            let path = ensure_env_file().map_err(|e| e.to_string())?;
            println!("{CURSOR_API_KEY}: 还没有。请编辑 {}", path.display());
        }
    }
    Ok(())
}

pub fn logout() -> Result<(), String> {
    let store = store()?;
    if store.remove(CURSOR_API_KEY).map_err(|e| e.to_string())? {
        println!("已从凭据文件删除 {CURSOR_API_KEY}");
    } else {
        println!("凭据文件里没有 {CURSOR_API_KEY}。若写在 .env 里，请直接清空那一行。");
    }
    Ok(())
}

pub fn cursor_key() -> Result<Option<livstyle_secrets::Secret>, String> {
    store()?.get(CURSOR_API_KEY).map_err(|e| e.to_string())
}
