use std::fmt;
use std::fs::OpenOptions;
use std::io::{self, Read};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::loader::Cancellation;
use markdown_linux_core::CoreUnavailability;

/// Sanitize a file or folder basename for UI display without leaking control or bidi characters.
pub fn sanitize_basename(path: &std::path::Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}') {
                '\u{fffd}'
            } else {
                c
            }
        })
        .take(160)
        .collect()
}

/// An explicit grant for one user-selected folder root.
#[derive(Clone)]
pub struct FolderCapability {
    root: PathBuf,
    revoked: Arc<AtomicBool>,
}
impl fmt::Debug for FolderCapability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("FolderCapability(<private>)")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FolderError {
    NotFound,
    PermissionDenied,
    AccessRevoked,
    VolumeUnavailable,
    NotDirectory,
    NonLocalFolder,
    OutsideFolder,
    ReadFailed,
}

impl fmt::Display for FolderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NotFound => "The folder is missing or was moved. Relocate it or choose another.",
            Self::PermissionDenied => {
                "Folder access is denied. Check permissions or re-grant access."
            }
            Self::AccessRevoked => {
                "Folder access was revoked. Open the folder again to grant access."
            }
            Self::VolumeUnavailable => "The storage is unavailable. Reconnect it and try again.",
            Self::NotDirectory => "Choose a folder, rather than a file.",
            Self::NonLocalFolder => "Only local folders are supported.",
            Self::OutsideFolder => "The selected file is outside the active workspace folder.",
            Self::ReadFailed => "The folder could not be opened. Check access and try again.",
        })
    }
}

impl FolderError {
    pub fn is_recoverable(self) -> bool {
        matches!(
            self,
            Self::NotFound | Self::PermissionDenied | Self::AccessRevoked | Self::VolumeUnavailable
        )
    }

    pub(super) fn io(error: io::Error) -> Self {
        match error.raw_os_error() {
            Some(2 | 20) => Self::NotFound, // ENOENT / ENOTDIR
            Some(1 | 13) => Self::PermissionDenied,
            Some(6 | 19 | 110 | 116) => Self::VolumeUnavailable,
            _ => Self::ReadFailed,
        }
    }
}

impl FolderCapability {
    pub fn from_local_folder(path: PathBuf) -> Result<Self, FolderError> {
        let cap = Self {
            root: path,
            revoked: Arc::new(AtomicBool::new(false)),
        };
        cap.validate()?;
        Ok(cap)
    }

    pub fn from_persisted(path: PathBuf, grant_active: bool) -> Self {
        Self {
            root: path,
            revoked: Arc::new(AtomicBool::new(!grant_active)),
        }
    }

    pub fn validate(&self) -> Result<(), FolderError> {
        if self.is_revoked() {
            return Err(FolderError::AccessRevoked);
        }
        let meta = std::fs::metadata(&self.root).map_err(FolderError::io)?;
        if !meta.is_dir() {
            return Err(FolderError::NotDirectory);
        }
        std::fs::read_dir(&self.root).map_err(FolderError::io)?;
        Ok(())
    }

    pub fn revoke(&self) {
        self.revoked.store(true, Ordering::Release);
    }

    pub fn is_revoked(&self) -> bool {
        self.revoked.load(Ordering::Acquire)
    }

    pub fn path(&self) -> &std::path::Path {
        &self.root
    }

    pub fn display_name(&self) -> String {
        sanitize_basename(&self.root)
    }

    pub fn contains_file(&self, file_path: &std::path::Path) -> Result<PathBuf, FolderError> {
        self.validate()?;
        let canonical_root = std::fs::canonicalize(&self.root).map_err(FolderError::io)?;
        let canonical_file = std::fs::canonicalize(file_path).map_err(FolderError::io)?;
        if !canonical_file.starts_with(&canonical_root) {
            return Err(FolderError::OutsideFolder);
        }
        Ok(canonical_file)
    }

    pub fn open_document(
        &self,
        file_path: &std::path::Path,
    ) -> Result<DocumentCapability, FolderError> {
        self.contains_file(file_path)?;
        Ok(DocumentCapability::from_local_file_with_folder(
            file_path.to_path_buf(),
            Some(self.clone()),
        ))
    }
}

/// An explicit grant for one selected path, including its symlink target. No
/// parent-directory authority is implied unless an explicit folder grant is attached.
/// Reload follows atomic replacement.
#[derive(Clone)]
pub struct DocumentCapability {
    path: PathBuf,
    revoked: Arc<AtomicBool>,
    folder: Option<FolderCapability>,
}
impl fmt::Debug for DocumentCapability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DocumentCapability(<private>)")
    }
}
impl DocumentCapability {
    pub fn from_local_file(path: PathBuf) -> Self {
        Self::from_local_file_with_folder(path, None)
    }
    pub fn from_local_file_with_folder(path: PathBuf, folder: Option<FolderCapability>) -> Self {
        Self {
            path,
            revoked: Arc::new(AtomicBool::new(false)),
            folder,
        }
    }
    pub fn revoke(&self) {
        self.revoked.store(true, Ordering::Release);
    }
    pub fn is_revoked(&self) -> bool {
        self.revoked.load(Ordering::Acquire)
            || self
                .folder
                .as_ref()
                .is_some_and(FolderCapability::is_revoked)
    }
    pub fn display_name(&self) -> String {
        sanitize_basename(&self.path)
    }
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
    pub fn folder(&self) -> Option<&FolderCapability> {
        self.folder.as_ref()
    }
    pub fn resources(&self) -> super::resources::DocumentResources {
        let folder = self
            .folder
            .as_ref()
            .map(|f| f.path().to_path_buf())
            .or_else(|| self.path.parent().map(std::path::Path::to_path_buf));
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

pub fn revision_token_status(cap: &DocumentCapability) -> Result<(), DocumentError> {
    revision_token(cap).map(|_| ())
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
    #[test]
    fn folder_capability_containment_revocation_and_recovery_errors() {
        use std::os::unix::fs::PermissionsExt;
        let (dir, doc_cap) = fixture();
        let folder = FolderCapability::from_local_folder(dir.clone()).unwrap();
        assert!(!format!("{folder:?}").contains("markdown-access"));
        assert!(!folder.display_name().is_empty());

        // Inside-folder document succeeds and inherits folder resources.
        let scoped = folder.open_document(doc_cap.path()).unwrap();
        assert_eq!(
            scoped.resources().folder(),
            std::fs::canonicalize(&dir).ok().as_deref()
        );

        // Outside-folder document is rejected with non-recoverable OutsideFolder.
        let outside = std::env::temp_dir().join(format!(
            "markdown-outside-{}-{}.md",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&outside, b"# Outside\n").unwrap();
        assert_eq!(
            folder.open_document(&outside).err(),
            Some(FolderError::OutsideFolder)
        );
        assert!(!FolderError::OutsideFolder.is_recoverable());
        std::fs::remove_file(&outside).unwrap();

        // NotDirectory error when pointing at a file.
        assert_eq!(
            FolderCapability::from_local_folder(doc_cap.path().to_path_buf()).err(),
            Some(FolderError::NotDirectory)
        );

        // Revocation propagates to folder and folder-scoped documents.
        folder.revoke();
        assert!(folder.is_revoked());
        assert!(scoped.is_revoked());
        assert_eq!(folder.validate().err(), Some(FolderError::AccessRevoked));
        assert!(FolderError::AccessRevoked.is_recoverable());

        // PermissionDenied on chmod 0o000 folder (non-root).
        let locked_dir = dir.join("locked");
        std::fs::create_dir(&locked_dir).unwrap();
        std::fs::set_permissions(&locked_dir, std::fs::Permissions::from_mode(0o0)).unwrap();
        let locked_res = FolderCapability::from_local_folder(locked_dir.clone());
        if std::process::Command::new("id")
            .arg("-u")
            .output()
            .unwrap()
            .stdout
            != b"0\n"
        {
            assert_eq!(locked_res.err(), Some(FolderError::PermissionDenied));
            assert!(FolderError::PermissionDenied.is_recoverable());
        }
        std::fs::set_permissions(&locked_dir, std::fs::Permissions::from_mode(0o755)).unwrap();

        // Moved/missing folder returns NotFound (recoverable).
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(
            FolderCapability::from_local_folder(dir).err(),
            Some(FolderError::NotFound)
        );
        assert!(FolderError::NotFound.is_recoverable());
    }
}
