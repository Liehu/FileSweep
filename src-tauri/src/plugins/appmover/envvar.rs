//! 环境变量备份/恢复（grill Q7 a/b）。
//!
//! 实现策略（MVP，稳健优先）：
//!   - 读取：用 `reg query` 导出整个 key 为文本，解析 KEY=VAL 行。
//!   - 备份：把解析结果存入 DB（保留 EXPAND_SZ 的类型标记）。
//!   - 恢复：逐条用 `reg add` 写回（带类型）。
//!   - 广播：用 PowerShell `[Environment]::SetEnvironmentVariable(..., $null)` 旁路或
//!     rundll32 触发广播。MVP 用 reg add 后提示用户重启生效。
//!
//! scope = user (HKCU\Environment) | system (HKLM\...\Session Manager\Environment)。

use rusqlite::Connection;

use crate::plugins::appmover::models::EnvBackupEntry;

/// 读取某 scope 的全部环境变量，返回 (key, value, is_expand) 三元组。
fn read_env_reg(scope: &str) -> Result<Vec<(String, String, bool)>, String> {
    let key = reg_key(scope)?;
    let out = std::process::Command::new("reg")
        .args(["query", &key])
        .output()
        .map_err(|e| format!("执行 reg query 失败: {}", e))?;
    if !out.status.success() {
        return Err(format!(
            "reg query 失败: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut result = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        // 行格式: "    PATH    REG_EXPAND_SZ    %SystemRoot%\..." 或 "    PATH    REG_SZ    ..."
        if line.is_empty() || line.starts_with("HKEY") {
            continue;
        }
        // 按 4 空格以上分割为三段
        let parts: Vec<&str> = line.splitn(3, |c: char| c.is_whitespace()).collect();
        if parts.len() < 3 {
            continue;
        }
        let name = parts[0].trim();
        let typ = parts[1].trim();
        // value 是剩余部分（splitn 3 已合并）
        let value = line[parts[0].len()..].trim_start();
        let value = value[typ.len()..].trim_start();
        if name.is_empty() {
            continue;
        }
        let is_expand = typ.eq_ignore_ascii_case("REG_EXPAND_SZ");
        result.push((name.to_string(), value.to_string(), is_expand));
    }
    Ok(result)
}

/// 备份某 scope 全部环境变量到 DB。返回写入条数。
pub fn backup_env(conn: &Connection, scope: &str) -> Result<usize, String> {
    let _ = match scope {
        "user" | "system" => scope,
        _ => return Err("scope 必须是 user 或 system".into()),
    };
    let vars = read_env_reg(scope)?;
    let now = chrono::Utc::now().timestamp();
    let mut count = 0;
    for (k, v, is_expand) in vars {
        // value 字段里用前缀标记类型："[E]..." 表示 EXPAND_SZ
        let stored = if is_expand { format!("[E]{}", v) } else { v };
        conn.execute(
            "INSERT INTO am_env_backup (scope, key, value, backed_up_at) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![scope, k, stored, now],
        )
        .map_err(|e| e.to_string())?;
        count += 1;
    }
    Ok(count)
}

/// 列出备份记录（按时间倒序）。
pub fn list_backups(conn: &Connection, scope: Option<&str>) -> rusqlite::Result<Vec<EnvBackupEntry>> {
    let sql = if scope.is_some() {
        "SELECT id, scope, key, value, backed_up_at FROM am_env_backup WHERE scope = ?1 ORDER BY backed_up_at DESC, key"
    } else {
        "SELECT id, scope, key, value, backed_up_at FROM am_env_backup ORDER BY backed_up_at DESC, key"
    };
    let mut stmt = conn.prepare(sql)?;
    let rows = if let Some(s) = scope {
        stmt.query_map(rusqlite::params![s], map_entry)?.collect::<rusqlite::Result<Vec<_>>>()?
    } else {
        stmt.query_map([], map_entry)?.collect::<rusqlite::Result<Vec<_>>>()?
    };
    Ok(rows)
}

fn map_entry(r: &rusqlite::Row) -> rusqlite::Result<EnvBackupEntry> {
    Ok(EnvBackupEntry {
        id: r.get(0)?,
        scope: r.get(1)?,
        key: r.get(2)?,
        value: r.get::<_, Option<String>>(3)?.unwrap_or_default(),
        backed_up_at: r.get(4)?,
    })
}

/// 恢复：把指定时间的备份组写回注册表。
pub fn restore_env(conn: &Connection, scope: &str, backed_up_at: i64) -> Result<usize, String> {
    let mut stmt = conn
        .prepare("SELECT key, value FROM am_env_backup WHERE scope = ?1 AND backed_up_at = ?2")
        .map_err(|e| e.to_string())?;
    let rows: Vec<(String, String)> = stmt
        .query_map(rusqlite::params![scope, backed_up_at], |r| {
            Ok((r.get(0)?, r.get::<_, Option<String>>(1)?.unwrap_or_default()))
        })
        .map_err(|e| e.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| e.to_string())?;
    if rows.is_empty() {
        return Err("未找到该备份记录".into());
    }
    let key = reg_key(scope)?;
    for (k, stored) in &rows {
        let (is_expand, v) = if let Some(rest) = stored.strip_prefix("[E]") {
            (true, rest)
        } else {
            (false, stored.as_str())
        };
        let typ = if is_expand { "REG_EXPAND_SZ" } else { "REG_SZ" };
        let status = std::process::Command::new("reg")
            .args([
                "add",
                &key,
                "/v",
                k,
                "/t",
                typ,
                "/d",
                v,
                "/f",
            ])
            .status()
            .map_err(|e| format!("reg add 失败: {}", e))?;
        if !status.success() {
            return Err(format!("恢复变量 {} 失败", k));
        }
    }
    // 广播（best-effort）
    let _ = broadcast_change();
    Ok(rows.len())
}

fn reg_key(scope: &str) -> Result<String, String> {
    match scope {
        "user" => Ok("HKCU\\Environment".into()),
        "system" => Ok(
            "HKLM\\SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Environment".into(),
        ),
        _ => Err("scope 必须是 user 或 system".into()),
    }
}

/// 广播环境变量变化（best-effort，失败不阻塞）。
fn broadcast_change() -> Result<(), String> {
    // 用 PowerShell 通知广播；失败忽略
    let script = r#"
Add-Type -Namespace Win32 -Name Native -MemberDefinition '[System.Runtime.InteropServices.DllImport("user32.dll", SetLastError = true, CharSet = System.Runtime.InteropServices.CharSet.Auto)] public static extern IntPtr SendMessageTimeout(IntPtr hWnd, uint Msg, IntPtr wParam, string lParam, uint fuFlags, uint uTimeout, out IntPtr lpdwResult);'
$h = [Win32.Native]::SendMessageTimeout([IntPtr]0xffff, 0x1a, [IntPtr]::Zero, 'Environment', 2, 5000, [ref][IntPtr]::Zero)
"#;
    let _ = std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", script])
        .status();
    Ok(())
}
