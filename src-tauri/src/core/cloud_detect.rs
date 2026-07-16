//! 云占位文件检测（OneDrive/iCloud Files On-Demand 等）
//!
//! OneDrive "文件随需下载"的占位符是 reparse point，本地只有几 KB 指针。
//! 如果对这类文件调用常规文件读取 API（如 partial hash 读头尾 4KB），
//! 会触发 OneDrive 客户端把云端文件强制下载到本地——
//! 轻则扫描巨慢，重则把用户网盘配额和本地磁盘写爆。
//!
//! 检测到云端占位文件时，应跳过内容哈希，仅用元数据（size + mtime）参与去重判断。

#[cfg(windows)]
mod windows_impl {
    use std::path::Path;

    /// FILE_ATTRIBUTE_OFFLINE：文件数据并非立即可用（云占位文件的常见标记）
    const FILE_ATTRIBUTE_OFFLINE: u32 = 0x00001000;
    /// FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS：访问数据时触发云端 recall（OneDrive 占位文件）
    const FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS: u32 = 0x00400000;
    /// FILE_ATTRIBUTE_REPARSE_POINT：reparse point（符号链接/云占位/其他重定向）
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x00000400;

    /// 判断文件是否为云端占位文件（读取会触发下载）。
    ///
    /// 检查 Windows 文件属性位：
    /// - OFFLINE：数据物理上不在此设备上
    /// - RECALL_ON_DATA_ACCESS：访问数据会触发从云端 recall
    /// - REPARSE_POINT（含 IO_REPARSE_TAG_CLOUD）：云重解析点
    ///
    /// 返回 true 表示该文件应跳过内容哈希，仅用元数据。
    pub fn is_cloud_placeholder(path: &Path) -> bool {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;

        // 转为 UTF-16 wide string（GetFileAttributesW 要求）
        let wide: Vec<u16> = OsStr::new(path)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        // SAFETY: GetFileAttributesW 读 wide string指针，以 null 结尾，无副作用
        let attrs = unsafe { winapi::um::fileapi::GetFileAttributesW(wide.as_ptr()) };

        // INVALID_FILE_ATTRIBUTES = 0xFFFFFFFF（u32::MAX）
        if attrs == winapi::um::fileapi::INVALID_FILE_ATTRIBUTES {
            return false; // 无法读取属性，不当作占位文件（避免误跳过）
        }

        (attrs & (FILE_ATTRIBUTE_OFFLINE | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS)) != 0
            || ((attrs & FILE_ATTRIBUTE_REPARSE_POINT) != 0
                && (attrs & FILE_ATTRIBUTE_OFFLINE) != 0)
    }
}

#[cfg(not(windows))]
mod other_impl {
    use std::path::Path;

    /// 非 Windows 平台：无 OneDrive 占位文件问题，始终返回 false
    pub fn is_cloud_placeholder(_path: &Path) -> bool {
        false
    }
}

#[cfg(windows)]
pub use windows_impl::is_cloud_placeholder;
#[cfg(not(windows))]
pub use other_impl::is_cloud_placeholder;

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_nonexistent_file_not_placeholder() {
        // 不存在的文件不应被判为占位文件（GetFileAttributesW 返回 INVALID）
        assert!(!is_cloud_placeholder(Path::new("Z:/nonexistent_file_12345.txt")));
    }
}
