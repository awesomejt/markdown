use std::fmt;
use std::fs::OpenOptions;
use std::io::{self, Read};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::loader::Cancellation;
use markdown_linux_core::CoreUnavailability;

/// An explicit grant for one selected path, including its symlink target. No
/// parent-directory authority is implied. Reload follows atomic replacement.
#[derive(Clone)]
pub struct DocumentCapability {
    path: PathBuf,
    revoked: Arc<AtomicBool>,
}
impl fmt::Debug for DocumentCapability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DocumentCapability(<private>)")
    }
}
impl DocumentCapability {
    pub fn from_local_file(path: PathBuf) -> Self {
        Self {
            path,
            revoked: Arc::new(AtomicBool::new(false)),
        }
    }
    pub fn revoke(&self) {
        self.revoked.store(true, Ordering::Release);
    }
    pub fn is_revoked(&self) -> bool {
        self.revoked.load(Ordering::Acquire)
    }
    pub fn display_name(&self) -> String {
        self.path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .chars()
            .map(|c| {
                if c.is_control() || matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
                {
                    '\u{fffd}'
                } else {
                    c
                }
            })
            .take(160)
            .collect()
    }
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
    pub fn resources(&self) -> super::resources::DocumentResources {
        let folder = self.path.parent().map(std::path::Path::to_path_buf);
        super::resources::DocumentResources::new(folder)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentError {
    NotFound,
    PermissionDenied,
    AccessRevoked,
    VolumeUnavailable,
    NotRegularFile,
    NonLocalFile,
    TooLarge,
    InvalidText,
    TooComplex,
    Busy,
    ReadFailed,
    ParseFailed,
}
impl fmt::Display for DocumentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NotFound => "The file is missing. Restore it or open another file.",
            Self::PermissionDenied => "Reading is denied. Check file permissions and reload.",
            Self::AccessRevoked => "File access was revoked. Open the file again to grant access.",
            Self::VolumeUnavailable => "The storage is unavailable. Reconnect it and reload.",
            Self::NotRegularFile => "Choose a regular text file, rather than a folder or device.",
            Self::NonLocalFile => "Only local files are supported. Save a local copy and open it.",
            Self::TooLarge => "The file exceeds the 5 MiB source limit. Open a smaller file.",
            Self::InvalidText => "The file is not UTF-8 text. Save a UTF-8 copy and open it.",
            Self::TooComplex => "The document exceeds the parser limits. Open a simpler document.",
            Self::Busy => "The parser is busy. Try Reload again.",
            Self::ReadFailed => "The file could not be read. Check access and try Reload.",
            Self::ParseFailed => "The document could not be parsed. Try another file.",
        })
    }
}
impl DocumentError {
    pub(super) fn unavailable(self) -> Option<CoreUnavailability> {
        Some(match self {
            Self::NotFound => CoreUnavailability::Deleted,
            Self::PermissionDenied => CoreUnavailability::PermissionDenied,
            Self::AccessRevoked => CoreUnavailability::AccessRevoked,
            Self::VolumeUnavailable | Self::ReadFailed => CoreUnavailability::Io,
            _ => return None,
        })
    }
    pub(super) fn io(error: io::Error) -> Self {
        match error.raw_os_error() {
            Some(2 | 20) => Self::NotFound, // ENOENT / ENOTDIR
            Some(1 | 13) => Self::PermissionDenied,
            Some(6 | 19 | 110 | 116) => Self::VolumeUnavailable, // ENXIO/ENODEV/ETIMEDOUT/ESTALE
            _ => Self::ReadFailed,
        }
    }
}
#[derive(Debug, PartialEq, Eq)]
pub(super) enum ReadFailure {
    Cancelled,
    Error(DocumentError),
}
impl From<io::Error> for ReadFailure {
    fn from(error: io::Error) -> Self {
        Self::Error(DocumentError::io(error))
    }
}
pub(super) struct ReadDocument {
    pub bytes: Vec<u8>,
    pub revision: Vec<u8>,
}
fn check(cap: &DocumentCapability, cancel: &Cancellation) -> Result<(), ReadFailure> {
    if cancel.is_cancelled() {
        return Err(ReadFailure::Cancelled);
    }
    if cap.is_revoked() {
        return Err(ReadFailure::Error(DocumentError::AccessRevoked));
    }
    Ok(())
}

pub(super) fn read_document(
    cap: &DocumentCapability,
    cancel: &Cancellation,
    limit: usize,
) -> Result<ReadDocument, ReadFailure> {
    check(cap, cancel)?;
    // Linux x86-64 development baseline: O_NONBLOCK | O_NOCTTY. Do not block
    // opening a FIFO that replaces the selected file between selection and open.
    // Other architectures require verification of their platform flag values.
    #[cfg(not(target_arch = "x86_64"))]
    compile_error!("Verify Linux O_NONBLOCK/O_NOCTTY flags for this architecture");
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(0x800 | 0x100)
        .open(&cap.path)?;
    let meta = file.metadata()?;
    if !meta.is_file() {
        return Err(ReadFailure::Error(DocumentError::NotRegularFile));
    }
    if meta.len() > limit as u64 {
        return Err(ReadFailure::Error(DocumentError::TooLarge));
    }
    let revision = format!(
        "{}:{}:{}:{}:{}:{}:{}",
        meta.dev(),
        meta.ino(),
        meta.len(),
        meta.mtime(),
        meta.mtime_nsec(),
        meta.ctime(),
        meta.ctime_nsec()
    )
    .into_bytes();
    let bytes = read_bounded(&mut file, cap, cancel, limit, meta.len() as usize + 1)?;
    Ok(ReadDocument { bytes, revision })
}

pub fn revision_token(cap: &DocumentCapability) -> Result<String, DocumentError> {
    if cap.is_revoked() {
        return Err(DocumentError::AccessRevoked);
    }
    let meta = std::fs::metadata(&cap.path).map_err(DocumentError::io)?;
    if !meta.is_file() {
        return Err(DocumentError::NotRegularFile);
    }
    use std::os::unix::fs::MetadataExt;
    Ok(format!(
        "{}:{}:{}:{}:{}:{}:{}",
        meta.dev(),
        meta.ino(),
        meta.len(),
        meta.mtime(),
        meta.mtime_nsec(),
        meta.ctime(),
        meta.ctime_nsec()
    ))
}
fn read_bounded(
    reader: &mut impl Read,
    cap: &DocumentCapability,
    cancel: &Cancellation,
    limit: usize,
    initial_capacity: usize,
) -> Result<Vec<u8>, ReadFailure> {
    // Size the initial allocation from descriptor metadata. Growth is explicit
    // so Vec doubling cannot exceed the source-plus-sentinel capacity bound.
    let mut bytes = Vec::with_capacity(initial_capacity.min(limit + 1));
    let mut chunk = [0u8; 64 * 1024];
    loop {
        check(cap, cancel)?;
        let count = chunk.len().min(limit + 1 - bytes.len());
        match reader.read(&mut chunk[..count]) {
            Ok(0) => break,
            Ok(n) => {
                let needed = bytes.len() + n;
                if needed > bytes.capacity() {
                    let capacity = bytes
                        .capacity()
                        .saturating_mul(2)
                        .max(needed)
                        .min(limit + 1);
                    bytes.reserve_exact(capacity - bytes.len());
                }
                bytes.extend_from_slice(&chunk[..n]);
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e.into()),
        }
        if bytes.len() > limit {
            return Err(ReadFailure::Error(DocumentError::TooLarge));
        }
    }
    check(cap, cancel)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::ffi::OsStringExt;
    use std::sync::atomic::AtomicU64;
    static NEXT: AtomicU64 = AtomicU64::new(0);
    pub(super) fn fixture() -> (PathBuf, DocumentCapability) {
        let dir = std::env::temp_dir().join(format!(
            "markdown-access-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join(std::ffi::OsString::from_vec(
            b"space \xff \xe6\x96\x87.any".to_vec(),
        ));
        std::fs::write(&path, b"# Selected\n").unwrap();
        (dir, DocumentCapability::from_local_file(path))
    }
    #[test]
    fn selected_file_and_revocation() {
        let (dir, cap) = fixture();
        let cancel = Cancellation::default();
        let read = read_document(&cap, &cancel, 100).unwrap();
        assert_eq!(read.bytes, b"# Selected\n");
        assert_eq!(read.bytes.capacity(), read.bytes.len() + 1);
        assert!(!format!("{cap:?}").contains("markdown-access"));
        cap.revoke();
        assert!(matches!(
            read_document(&cap, &cancel, 100),
            Err(ReadFailure::Error(DocumentError::AccessRevoked))
        ));
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn real_descriptor_kinds_and_limits() {
        let (dir, cap) = fixture();
        let cancel = Cancellation::default();
        assert!(matches!(
            read_document(&cap, &cancel, 2),
            Err(ReadFailure::Error(DocumentError::TooLarge))
        ));
        let directory = DocumentCapability::from_local_file(dir.clone());
        assert!(matches!(
            read_document(&directory, &cancel, 100),
            Err(ReadFailure::Error(DocumentError::NotRegularFile))
        ));
        let fifo = dir.join("pipe");
        assert!(
            std::process::Command::new("mkfifo")
                .arg(&fifo)
                .status()
                .unwrap()
                .success()
        );
        assert!(matches!(
            read_document(&DocumentCapability::from_local_file(fifo), &cancel, 100),
            Err(ReadFailure::Error(DocumentError::NotRegularFile))
        ));
        let device = DocumentCapability::from_local_file(PathBuf::from("/dev/null"));
        assert!(matches!(
            read_document(&device, &cancel, 100),
            Err(ReadFailure::Error(DocumentError::NotRegularFile))
        ));
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn bounded_growth_and_cancel_between_chunks() {
        let (_, cap) = fixture();
        let cancel = Cancellation::default();
        let mut growing = io::repeat(b'x');
        assert_eq!(
            read_bounded(&mut growing, &cap, &cancel, 12, 0),
            Err(ReadFailure::Error(DocumentError::TooLarge))
        );
        struct Revoking<'a>(&'a DocumentCapability);
        impl Read for Revoking<'_> {
            fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                self.0.revoke();
                buf[0] = b'x';
                Ok(1)
            }
        }
        assert_eq!(
            read_bounded(&mut Revoking(&cap), &cap, &cancel, 12, 0),
            Err(ReadFailure::Error(DocumentError::AccessRevoked))
        );
        std::fs::remove_dir_all(cap.path.parent().unwrap()).unwrap();
    }
    #[test]
    fn permission_denial_and_simulated_unavailable() {
        use std::os::unix::fs::PermissionsExt;
        let (dir, cap) = fixture();
        std::fs::set_permissions(&cap.path, std::fs::Permissions::from_mode(0o0)).unwrap();
        let result = read_document(&cap, &Cancellation::default(), 100);
        if std::process::Command::new("id")
            .arg("-u")
            .output()
            .unwrap()
            .stdout
            != b"0\n"
        {
            assert!(matches!(
                result,
                Err(ReadFailure::Error(DocumentError::PermissionDenied))
            ));
        }
        for errno in [6, 19, 110, 116] {
            assert_eq!(
                DocumentError::io(io::Error::from_raw_os_error(errno)),
                DocumentError::VolumeUnavailable
            );
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
