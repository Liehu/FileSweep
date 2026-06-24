//! Everything 搜索集成
//!
//! 通过 es.exe（Everything CLI）实现极速文件搜索。
//! 运行时要求：用户安装 Everything + ES CLI（es.exe 在 PATH 中）。
//!
//! 降级策略：如果 es.exe 不可用，回退到 DB 内搜索（get_file_records 的 search 参数）。

use std::process::Command;

#[derive(Debug, Clone, serde::Serialize)]
pub struct SearchResult {
    pub name: String,
    pub path: String,
    pub size: i64,
}

/// 检测 es.exe 是否可用
pub fn is_everything_available() -> bool {
    Command::new("es.exe")
        .arg("-version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// 通过 Everything 搜索文件
///
/// query: Everything 搜索语法（如 "python .exe"）
/// max_results: 最多返回结果数
pub fn search_with_everything(query: &str, max_results: usize) -> Result<Vec<SearchResult>, String> {
    if !is_everything_available() {
        return Err("Everything (es.exe) 未安装或不在 PATH 中".into());
    }

    let output = Command::new("es.exe")
        .args([
            "-n", &max_results.to_string(), // 限制结果数
            "-size",                         // 输出文件大小
            "-path-column",                  // 输出完整路径
            query,
        ])
        .output()
        .map_err(|e| format!("执行 es.exe 失败: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("es.exe 搜索失败: {}", stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut results = Vec::new();

    for line in stdout.lines() {
        // es.exe -size -path-column 输出格式：path\tsize
        // 或简单格式：path（每行一个路径）
        let path = line.trim();
        if path.is_empty() {
            continue;
        }

        // 尝试解析 size（如果有 tab 分隔）
        let (path_str, size) = if let Some(tab_idx) = path.find('\t') {
            let p = &path[..tab_idx];
            let s = path[tab_idx + 1..].trim();
            (p.to_string(), s.parse::<i64>().unwrap_or(-1))
        } else {
            (path.to_string(), -1i64)
        };

        // 提取文件名
        let name = std::path::Path::new(&path_str)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path_str.clone());

        results.push(SearchResult {
            name,
            path: path_str,
            size,
        });
    }

    Ok(results)
}
