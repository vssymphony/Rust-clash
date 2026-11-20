//! `wintun.dll` 的定位与静态校验。
//!
//! # 为什么单独成模块
//!
//! mihomo 在 Windows 上启用 TUN 需要 `wintun.dll`。它的加载路径直接决定
//! 是否存在 **DLL 劫持 / DLL 预加载** 面：只要搜索路径里出现用户可写、
//! 但程序不掌控的目录（典型是 `%APPDATA%` 根目录、当前工作目录、PATH），
//! 同机低权限进程就能放一个同名 DLL 让本程序加载。
//!
//! 因此这里做两件事：
//!
//! 1. **把搜索路径收敛为两级**：仅 `exe` 所在目录与应用数据目录；
//!    其余一律拒绝（含 `%APPDATA%` 根、`%LOCALAPPDATA%`、`%TEMP%`、
//!    `System32`、当前工作目录）。
//! 2. **加载前做静态校验**：文件大小区间 + PE 头 + 目标架构 + 版本号。
//!
//! # 签名校验为什么没做（如实说明 + 降级方案）
//!
//! Authenticode 校验需要调用 `WinVerifyTrust`（`wintrust.dll`）或
//! `windows`/`winapi` crate 的 `WinVerifyTrust` 封装，并解析
//! `WINTRUST_DATA` / `WINTRUST_FILE_INFO` 结构。当前开发机的
//! **Smart App Control 强制开启且 Rust 工具链已清理**，无法编译验证
//! 任何真正调到 FFI 的代码；写一段"看起来对但编不过、或编过却传错
//! 结构体字段"的校验代码，比不做校验更危险（会给出虚假的安全感）。
//!
//! 降级方案分三层，本模块负责前两层：
//!
//! | 层 | 手段 | 强度 |
//! |---|---|---|
//! | L1 | 搜索路径收敛（仅两级目录） | 消除绝大部分劫持面 |
//! | L2 | PE 头 + 架构 + 文件大小 + 文件版本 | 拦截"放错架构/放了个假文件/版本过旧" |
//! | L3 | 发布前用 `scripts/verify-wintun-sign.ps1` 调用系统 `Get-AuthenticodeSignature` 校验发行者 | 需要人工/CI 执行 |
//!
//! L3 不放在运行时，是因为它要么依赖 PowerShell（每次启动拉起一个
//! 进程、且 PowerShell 本身可被策略禁用），要么依赖无法在本机验证的 FFI。
//! 取舍写在这里，避免后来者误以为"已经验过签名"。

use std::path::{Path, PathBuf};

/// 依赖文件名。
pub const DLL_FILE_NAME: &str = "wintun.dll";

/// 允许的 DLL 大小下限（字节）。官方 0.14.x amd64 约 400 KiB，
/// 取 200 KiB 作为下界：既能拦住"放了个 README 改名"的误操作，
/// 又不会因为上游精简发行版而误报。
pub const MIN_DLL_BYTES: u64 = 200 * 1024;

/// 允许的 DLL 大小上限（字节）。4 MiB 足够覆盖各架构带调试符号的发行版。
pub const MAX_DLL_BYTES: u64 = 4 * 1024 * 1024;

/// 建议的最低 wintun 版本（0.14 起 API 稳定、且修复了 Win10 20H1 前的
/// 若干蓝屏问题）。
pub const RECOMMENDED_MIN_VERSION: (u16, u16) = (0, 14);

const MACHINE_I386: u16 = 0x014c;
const MACHINE_AMD64: u16 = 0x8664;
const MACHINE_ARM64: u16 = 0xaa64;

const PE32_MAGIC: u16 = 0x010b;
const PE32_PLUS_MAGIC: u16 = 0x020b;

/// 校验失败原因。每个分支都带足够信息，便于前端直接展示。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WintunError {
    /// 候选目录里都没有 `wintun.dll`。
    NotFound,
    /// 文件过小（多半是下载了 HTML 错误页或占位文件）。
    TooSmall(u64),
    /// 文件过大（误放了安装包 / 压缩包）。
    TooLarge(u64),
    /// 不是合法的 PE 文件（缺少 `MZ` 头）。
    BadDosHeader,
    /// `PE\0\0` 签名或 optional header 缺失。
    BadPeHeader,
    /// 架构与宿主不匹配（例如 32 位 DLL 喂给 64 位内核）。
    MachineMismatch { found: u16, expected: u16 },
    /// 版本号低于建议值。
    VersionTooOld { major: u16, minor: u16 },
    /// 读取文件失败。
    Io(String),
}

impl WintunError {
    /// 面向用户的中文说明。
    pub fn message(&self) -> String {
        match self {
            WintunError::NotFound => "未在程序目录或应用数据目录找到 wintun.dll".to_string(),
            WintunError::TooSmall(size) => format!(
                "wintun.dll 只有 {size} 字节，明显不是有效的驱动库（可能是下载到的错误页或占位文件），\
                 请重新从 https://www.wintun.net/ 下载"
            ),
            WintunError::TooLarge(size) => format!(
                "wintun.dll 体积为 {size} 字节，超出合理区间（>{} MiB），\
                 可能误把压缩包当成 DLL，请解压后只放 wintun.dll",
                MAX_DLL_BYTES / 1024 / 1024
            ),
            WintunError::BadDosHeader => {
                "wintun.dll 不是合法的 PE 文件（缺少 MZ 头），请重新下载".to_string()
            }
            WintunError::BadPeHeader => {
                "wintun.dll 的 PE 头损坏（缺少 PE 签名或 optional header），请重新下载".to_string()
            }
            WintunError::MachineMismatch { found, expected } => format!(
                "wintun.dll 架构不匹配：文件为 {}，本机需要 {}。\
                 请下载与本程序一致（通常为 amd64）的版本",
                machine_name(*found),
                machine_name(*expected)
            ),
            WintunError::VersionTooOld { major, minor } => format!(
                "wintun.dll 版本过旧（{major}.{minor}），建议 {} 及以上版本",
                format_version(RECOMMENDED_MIN_VERSION)
            ),
            WintunError::Io(message) => format!("读取 wintun.dll 失败：{message}"),
        }
    }
}

/// PE 镜像的基本信息。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageInfo {
    /// PE `Machine` 字段。
    pub machine: u16,
    /// optional header 是否为 PE32+。
    pub is_64bit: bool,
    /// 从 `VS_FIXEDFILEINFO` 提取到的文件版本，提取不到时为 `None`。
    pub version: Option<(u16, u16)>,
}

impl ImageInfo {
    /// 版本的可读形式（提取不到时返回 `未知`）。
    pub fn version_text(&self) -> String {
        match self.version {
            Some(version) => format_version(version),
            None => "未知".to_string(),
        }
    }

    /// 架构的可读形式。
    pub fn machine_text(&self) -> String {
        machine_name(self.machine).to_string()
    }
}

/// `major.minor` 格式化。
pub fn format_version(version: (u16, u16)) -> String {
    format!("{}.{}", version.0, version.1)
}

fn machine_name(machine: u16) -> &'static str {
    match machine {
        MACHINE_I386 => "x86",
        MACHINE_AMD64 => "amd64",
        MACHINE_ARM64 => "arm64",
        _ => "未知架构",
    }
}

/// 当前宿主需要的 PE `Machine`。
///
/// mihomo sidecar 由 `prepare-sidecar` 按 host triple 下载，因此 DLL 架构
/// 必须与宿主进程一致，而不是与"内核参数"一致。
pub fn expected_machine() -> u16 {
    if cfg!(target_arch = "aarch64") {
        MACHINE_ARM64
    } else if cfg!(target_arch = "x86") {
        MACHINE_I386
    } else {
        MACHINE_AMD64
    }
}

fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    let slice = bytes.get(offset..offset + 2)?;
    Some(u16::from_le_bytes([slice[0], slice[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let slice = bytes.get(offset..offset + 4)?;
    Some(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

/// 纯字节级校验（便于在任意平台单测，不需要真的存在 DLL）。
pub fn validate_bytes(bytes: &[u8]) -> Result<ImageInfo, WintunError> {
    if bytes.len() < 0x40 || !bytes.starts_with(b"MZ") {
        return Err(WintunError::BadDosHeader);
    }

    let pe_offset = read_u32(bytes, 0x3c).ok_or(WintunError::BadPeHeader)? as usize;
    // 至少要有 COFF header(20 字节) + optional header magic(2 字节)
    if pe_offset + 24 > bytes.len() || &bytes[pe_offset..pe_offset + 4] != b"PE\0\0" {
        return Err(WintunError::BadPeHeader);
    }

    let machine = read_u16(bytes, pe_offset + 4).ok_or(WintunError::BadPeHeader)?;
    let optional_magic = read_u16(bytes, pe_offset + 24).ok_or(WintunError::BadPeHeader)?;
    if optional_magic != PE32_MAGIC && optional_magic != PE32_PLUS_MAGIC {
        return Err(WintunError::BadPeHeader);
    }

    let expected = expected_machine();
    if machine != expected {
        return Err(WintunError::MachineMismatch {
            found: machine,
            expected,
        });
    }

    Ok(ImageInfo {
        machine,
        is_64bit: optional_magic == PE32_PLUS_MAGIC,
        version: find_fixed_file_version(bytes),
    })
}

/// 在字节流中查找 `VS_FIXEDFILEINFO`，返回 `(major, minor)`。
///
/// `VS_FIXEDFILEINFO` 以固定签名 `0xFEEF04BD` 开头，随后是
/// `dwStrucVersion`（恒为 0x00010000），再往后才是 `dwFileVersionMS/LS`。
/// 用签名 + strucVersion 双重校验，可以把"偶然出现的同样 4 字节"过滤掉。
pub fn find_fixed_file_version(bytes: &[u8]) -> Option<(u16, u16)> {
    const SIGNATURE: [u8; 4] = [0xbd, 0x04, 0xef, 0xfe];
    let mut index = 0usize;
    while index + 16 <= bytes.len() {
        if bytes[index..index + 4] == SIGNATURE {
            let struc_version = read_u32(bytes, index + 4);
            if struc_version == Some(0x0001_0000) {
                if let Some(ms) = read_u32(bytes, index + 8) {
                    let major = (ms >> 16) as u16;
                    let minor = (ms & 0xffff) as u16;
                    // wintun 版本号一定是 0.x / 1.x 这种量级，超过 99 基本是误命中
                    if major > 0 && major < 100 {
                        return Some((major, minor));
                    }
                }
            }
        }
        index += 1;
    }
    None
}

/// 版本门槛校验：提取不到版本时**放行**（L2 已通过就足够），
/// 提取到且低于建议值时拒绝。
pub fn ensure_version(info: &ImageInfo) -> Result<(), WintunError> {
    match info.version {
        None => Ok(()),
        Some((major, minor)) if (major, minor) >= RECOMMENDED_MIN_VERSION => Ok(()),
        Some((major, minor)) => Err(WintunError::VersionTooOld { major, minor }),
    }
}

/// 校验磁盘上的一个 DLL 文件（大小 → PE → 架构 → 版本）。
pub fn validate_file(path: &Path) -> Result<ImageInfo, WintunError> {
    let metadata = std::fs::metadata(path).map_err(|error| WintunError::Io(error.to_string()))?;
    if !metadata.is_file() {
        return Err(WintunError::NotFound);
    }

    let size = metadata.len();
    if size < MIN_DLL_BYTES {
        return Err(WintunError::TooSmall(size));
    }
    if size > MAX_DLL_BYTES {
        return Err(WintunError::TooLarge(size));
    }

    let bytes = std::fs::read(path).map_err(|error| WintunError::Io(error.to_string()))?;
    let info = validate_bytes(&bytes)?;
    ensure_version(&info)?;
    Ok(info)
}

/// 判断某个目录是否**禁止**作为 DLL 搜索目录。
///
/// 命中以下任一即拒绝：
/// * 用户漫游 / 本地 AppData 根、用户主目录、临时目录、ProgramData；
/// * `System32` / `SysWOW64`（系统目录不属于本应用的投放位置）；
/// * 进程当前工作目录（可能是用户从任意目录启动程序时继承的）。
pub fn is_forbidden_dir(dir: &Path) -> bool {
    if dir.as_os_str().is_empty() {
        return true;
    }

    let text = normalize(dir.to_string_lossy().as_ref());

    for variable in ["APPDATA", "LOCALAPPDATA", "USERPROFILE", "TEMP", "TMP", "PROGRAMDATA"] {
        if let Ok(value) = std::env::var(variable) {
            let normalized = normalize(&value);
            if !normalized.is_empty() && text == normalized {
                return true;
            }
        }
    }

    if text.contains("\\windows\\system32") || text.contains("\\windows\\syswow64") {
        return true;
    }

    if let Ok(cwd) = std::env::current_dir() {
        if normalize(cwd.to_string_lossy().as_ref()) == text {
            return true;
        }
    }

    false
}

fn normalize(path: &str) -> String {
    path.replace('/', "\\")
        .trim_end_matches('\\')
        .to_ascii_lowercase()
}

/// 生成候选文件路径列表（顺序即优先级）：exe 目录 → 应用数据目录。
///
/// 只接受这两级；`%APPDATA%` 根等高风险目录由 [`is_forbidden_dir`] 拦掉。
pub fn candidate_files(exe_dir: Option<&Path>, data_dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = Vec::new();
    let candidates = [exe_dir.map(Path::to_path_buf), Some(data_dir.to_path_buf())];

    for dir in candidates.into_iter().flatten() {
        if is_forbidden_dir(&dir) {
            continue;
        }
        let file = dir.join(DLL_FILE_NAME);
        if !files.contains(&file) {
            files.push(file);
        }
    }

    files
}

/// 把候选路径列表格式化成一行，便于写进日志/诊断详情。
pub fn describe_candidates(files: &[PathBuf]) -> String {
    if files.is_empty() {
        return "无可用的 wintun.dll 候选目录（已排除系统目录与用户漫游根目录）".to_string();
    }
    files
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<String>>()
        .join(" / ")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造一个最小但结构合法的 PE 镜像（仅覆盖校验需要的字段）。
    fn minimal_pe(machine: u16, pe32_plus: bool) -> Vec<u8> {
        let mut bytes = vec![0u8; 0x200];
        bytes[0] = b'M';
        bytes[1] = b'Z';
        let pe_offset: u32 = 0x80;
        bytes[0x3c..0x40].copy_from_slice(&pe_offset.to_le_bytes());
        let offset = pe_offset as usize;
        bytes[offset..offset + 4].copy_from_slice(b"PE\0\0");
        bytes[offset + 4..offset + 6].copy_from_slice(&machine.to_le_bytes());
        let magic = if pe32_plus { PE32_PLUS_MAGIC } else { PE32_MAGIC };
        bytes[offset + 24..offset + 26].copy_from_slice(&magic.to_le_bytes());
        bytes
    }

    fn append_version(mut bytes: Vec<u8>, major: u16, minor: u16) -> Vec<u8> {
        let mut block = Vec::new();
        block.extend_from_slice(&[0xbd, 0x04, 0xef, 0xfe]); // dwSignature
        block.extend_from_slice(&0x0001_0000u32.to_le_bytes()); // dwStrucVersion
        let ms = ((major as u32) << 16) | minor as u32;
        block.extend_from_slice(&ms.to_le_bytes()); // dwFileVersionMS
        block.extend_from_slice(&0u32.to_le_bytes()); // dwFileVersionLS
        bytes.extend_from_slice(&block);
        bytes
    }

    #[test]
    fn accepts_valid_image_with_expected_arch() {
        let bytes = minimal_pe(expected_machine(), true);
        let info = validate_bytes(&bytes).expect("应能通过校验");
        assert_eq!(info.machine, expected_machine());
        assert!(info.is_64bit);
        assert_eq!(info.version, None);
    }

    #[test]
    fn rejects_non_pe_bytes() {
        assert_eq!(validate_bytes(b"not a dll"), Err(WintunError::BadDosHeader));
        let mut bytes = minimal_pe(expected_machine(), true);
        bytes[0] = 0;
        assert_eq!(validate_bytes(&bytes), Err(WintunError::BadDosHeader));
    }

    #[test]
    fn rejects_broken_pe_signature() {
        let mut bytes = minimal_pe(expected_machine(), true);
        bytes[0x80] = b'X';
        assert_eq!(validate_bytes(&bytes), Err(WintunError::BadPeHeader));
    }

    #[test]
    fn rejects_wrong_architecture() {
        let wrong = if expected_machine() == MACHINE_I386 {
            MACHINE_AMD64
        } else {
            MACHINE_I386
        };
        let bytes = minimal_pe(wrong, false);
        match validate_bytes(&bytes) {
            Err(WintunError::MachineMismatch { found, expected }) => {
                assert_eq!(found, wrong);
                assert_eq!(expected, expected_machine());
            }
            other => panic!("应判定架构不匹配，实际 {other:?}"),
        }
    }

    #[test]
    fn extracts_file_version_from_fixed_file_info() {
        let bytes = append_version(minimal_pe(expected_machine(), true), 0, 14);
        assert_eq!(find_fixed_file_version(&bytes), Some((0, 14)));

        let info = validate_bytes(&bytes).expect("应能通过校验");
        assert_eq!(info.version, Some((0, 14)));
        assert_eq!(info.version_text(), "0.14");
        assert!(ensure_version(&info).is_ok());
    }

    #[test]
    fn rejects_too_old_version_but_allows_unknown() {
        let old = ImageInfo {
            machine: expected_machine(),
            is_64bit: true,
            version: Some((0, 11)),
        };
        assert_eq!(
            ensure_version(&old),
            Err(WintunError::VersionTooOld {
                major: 0,
                minor: 11
            })
        );

        let unknown = ImageInfo {
            machine: expected_machine(),
            is_64bit: true,
            version: None,
        };
        assert!(ensure_version(&unknown).is_ok(), "版本不可提取时应降级放行");
    }

    #[test]
    fn ignores_incidental_signature_like_bytes() {
        let mut bytes = minimal_pe(expected_machine(), true);
        // 伪造签名但 strucVersion 不对，应被忽略
        bytes.extend_from_slice(&[0xbd, 0x04, 0xef, 0xfe]);
        bytes.extend_from_slice(&0x0002_0000u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        assert_eq!(find_fixed_file_version(&bytes), None);
    }

    #[test]
    fn candidate_files_prefer_exe_dir_and_drop_duplicates() {
        let exe_dir = PathBuf::from(r"C:\Program Files\clash-gui");
        let data_dir = PathBuf::from(r"C:\Users\Public\clash-gui\data");
        let files = candidate_files(Some(&exe_dir), &data_dir);
        assert_eq!(files.len(), 2, "两级目录各一个候选：{files:?}");
        assert_eq!(files[0], exe_dir.join(DLL_FILE_NAME));
        assert_eq!(files[1], data_dir.join(DLL_FILE_NAME));

        // 同一目录出现两次时只保留一个
        let deduped = candidate_files(Some(&data_dir), &data_dir);
        assert_eq!(deduped.len(), 1);
    }

    #[test]
    fn rejects_appdata_root_as_search_dir() {
        if let Ok(appdata) = std::env::var("APPDATA") {
            assert!(
                is_forbidden_dir(Path::new(&appdata)),
                "%APPDATA% 根目录必须被拒绝：{appdata}"
            );
            let private = Path::new(&appdata).join("com.clashgui.desktop\\data");
            assert!(
                !is_forbidden_dir(&private),
                "应用私有子目录应允许：{}",
                private.display()
            );
        }
    }

    #[test]
    fn rejects_empty_and_system_dirs() {
        assert!(is_forbidden_dir(Path::new("")));
        assert!(is_forbidden_dir(Path::new(r"C:\Windows\System32")));
        assert!(is_forbidden_dir(Path::new(r"C:\Windows\SysWOW64")));
    }

    #[test]
    fn describe_candidates_is_readable() {
        let files = vec![PathBuf::from(r"C:\a\wintun.dll"), PathBuf::from(r"C:\b\wintun.dll")];
        let text = describe_candidates(&files);
        assert!(text.contains(r"C:\a\wintun.dll"), "{text}");
        assert!(text.contains(r"C:\b\wintun.dll"), "{text}");
        assert!(describe_candidates(&[]).contains("无可用的"));
    }
}
