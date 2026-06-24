//! Uninstall 注册表只读枚举（grill Q7 c）。
//!
//! 只读展示已安装程序列表，不做备份/恢复。
//! 枚举 HKLM 64/32 位 + HKCU 的 Uninstall 子键。

use crate::plugins::appmover::models::UninstallEntry;

/// 列出所有已安装程序（合并 HKLM/HKLM-WOW64/HKCU 的 Uninstall 项）。
pub fn list_installed() -> Vec<UninstallEntry> {
    let mut out = Vec::new();
    for root in [
        "HKLM\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
        "HKLM\\SOFTWARE\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
        "HKCU\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
    ] {
        let subs = enum_subkeys(root).unwrap_or_default();
        for sub in subs {
            let full = format!("{}\\{}", root, sub);
            let values = query_values(&full);
            let name = values.get("DisplayName").cloned().unwrap_or_default();
            if name.is_empty() {
                continue;
            }
            out.push(UninstallEntry {
                name,
                version: values.get("DisplayVersion").cloned().unwrap_or_default(),
                publisher: values.get("Publisher").cloned().unwrap_or_default(),
                install_location: values.get("InstallLocation").cloned().unwrap_or_default(),
                uninstall_string: values.get("UninstallString").cloned().unwrap_or_default(),
            });
        }
    }
    // 按名称去重（同软件可能在多个 root 注册）
    out.sort_by(|a, b| a.name.to_ascii_lowercase().cmp(&b.name.to_ascii_lowercase()));
    out.dedup_by(|a, b| a.name.eq_ignore_ascii_case(&b.name));
    out
}

/// 枚举某 key 的子键名。
fn enum_subkeys(key: &str) -> Result<Vec<String>, ()> {
    let out = std::process::Command::new("reg")
        .args(["query", key])
        .output()
        .map_err(|_| ())?;
    if !out.status.success() {
        return Ok(Vec::new());
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut subs = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with("HKEY") {
            // 取最后一段作为子键名
            if let Some(name) = line.rsplit(['\\', '/']).next() {
                let n = name.trim();
                if !n.is_empty() {
                    subs.push(n.to_string());
                }
            }
        }
    }
    Ok(subs)
}

/// 查询某 key 下所有值名→字符串值。
fn query_values(key: &str) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    let out = match std::process::Command::new("reg").args(["query", key]).output() {
        Ok(o) => o,
        Err(_) => return map,
    };
    if !out.status.success() {
        return map;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("HKEY") {
            continue;
        }
        // 格式: "    Name    REG_SZ    value"
        let parts: Vec<&str> = line.splitn(3, char::is_whitespace).collect();
        if parts.len() < 3 {
            continue;
        }
        let name = parts[0].trim();
        let typ = parts[1].trim();
        if !typ.eq_ignore_ascii_case("REG_SZ")
            && !typ.eq_ignore_ascii_case("REG_EXPAND_SZ")
            && !typ.eq_ignore_ascii_case("REG_MULTI_SZ")
        {
            continue;
        }
        let value = line[parts[0].len()..].trim_start();
        let value = value[typ.len()..].trim_start();
        if !name.is_empty() {
            map.insert(name.to_string(), value.to_string());
        }
    }
    map
}
