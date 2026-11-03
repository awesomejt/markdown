use std::fmt;
use std::fs::OpenOptions;
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

/// Why an image resource beside the document was not loaded.
/// User-facing messages name no path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceError {
    OutsideFolder,
    NotFound,
    PermissionDenied,
    NotRegularFile,
    UnsupportedType,
    TooLarge,
    Limit,
    Undecodable,
    Remote,
    Blocked,
    NotLoaded,
}

impl ResourceError {
    pub fn message(&self) -> &'static str {
        match self {
            Self::Remote => "Remote images are blocked",
            Self::Blocked => "This image was blocked",
            Self::NotLoaded => "Image not shown",
            Self::OutsideFolder => "Images outside the document’s folder are not shown",
            Self::NotFound => "Image not found",
            Self::PermissionDenied => "You don’t have permission to read this image",
            Self::NotRegularFile => "This image file is not a regular file",
            Self::UnsupportedType => "This image format is not supported",
            Self::TooLarge => "This image is too large to show",
            Self::Limit => "Too many images to show",
            Self::Undecodable => "This image couldn’t be read",
        }
    }
}

impl fmt::Display for ResourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message())
    }
}

impl std::error::Error for ResourceError {}

/// Per-document image bounds. Decoding is bounded before any pixels exist:
/// files are size-capped and declared dimensions are verified from headers.
#[derive(Clone, Debug)]
pub struct ImageBudget {
    pub max_images: usize,
    pub max_file_bytes: usize,
    pub max_source_pixels: u64,
    pub max_pixel_side: u32,
    pub max_decoded_bytes: usize,
}

impl Default for ImageBudget {
    fn default() -> Self {
        Self {
            max_images: 128,
            max_file_bytes: 16 * 1024 * 1024,    // 16 MiB
            max_source_pixels: 32 * 1024 * 1024, // 32 MP
            max_pixel_side: 2048,
            max_decoded_bytes: 128 * 1024 * 1024, // 128 MiB
        }
    }
}

/// Safe image format and dimension inspector.
/// Validates magic bytes, extracts dimensions, and detects SVG / XML without allocating full image surfaces.
pub struct ImageFormatInspector;

impl ImageFormatInspector {
    pub fn inspect(
        data: &[u8],
        max_source_pixels: u64,
        max_pixel_side: u32,
    ) -> Result<(usize, usize), ResourceError> {
        if data.len() < 4 {
            return Err(ResourceError::Undecodable);
        }

        // SVG / XML detection: reject as unsupported vector format
        if Self::is_svg_or_xml(data) {
            return Err(ResourceError::UnsupportedType);
        }

        // 1. PNG: 89 50 4E 47 0D 0A 1A 0A
        if data.len() >= 24
            && data[0] == 0x89
            && data[1] == 0x50
            && data[2] == 0x4E
            && data[3] == 0x47
            && data[4] == 0x0D
            && data[5] == 0x0A
            && data[6] == 0x1A
            && data[7] == 0x0A
        {
            // First chunk must be IHDR at offset 12
            if &data[12..16] == b"IHDR" {
                let w = u32::from_be_bytes([data[16], data[17], data[18], data[19]]) as u64;
                let h = u32::from_be_bytes([data[20], data[21], data[22], data[23]]) as u64;
                return Self::validate_dimensions(w, h, max_source_pixels, max_pixel_side);
            }
            return Err(ResourceError::Undecodable);
        }

        // 2. JPEG: FF D8 FF
        if data.len() >= 4 && data[0] == 0xFF && data[1] == 0xD8 && data[2] == 0xFF {
            let mut offset = 2;
            while offset + 4 < data.len() {
                if data[offset] != 0xFF {
                    break;
                }
                while offset < data.len() && data[offset] == 0xFF {
                    offset += 1;
                }
                if offset >= data.len() {
                    break;
                }
                let marker = data[offset];
                offset += 1;

                // SOF markers: C0..=C3, C5..=C7, C9..=CB, CD..=CF
                if matches!(
                    marker,
                    0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF
                ) {
                    if offset + 7 <= data.len() {
                        let h = u16::from_be_bytes([data[offset + 3], data[offset + 4]]) as u64;
                        let w = u16::from_be_bytes([data[offset + 5], data[offset + 6]]) as u64;
                        return Self::validate_dimensions(w, h, max_source_pixels, max_pixel_side);
                    }
                    break;
                }

                // EOI or SOS: stop
                if marker == 0xD9 || marker == 0xDA {
                    break;
                }

                if offset + 2 > data.len() {
                    break;
                }
                let len = u16::from_be_bytes([data[offset], data[offset + 1]]) as usize;
                if len < 2 {
                    break;
                }
                offset += len;
            }
            return Err(ResourceError::Undecodable);
        }

        // 3. GIF: GIF87a or GIF89a
        if data.len() >= 10
            && &data[0..3] == b"GIF"
            && data[3] == b'8'
            && (data[4] == b'7' || data[4] == b'8' || data[4] == b'9')
            && data[5] == b'a'
        {
            let w = u16::from_le_bytes([data[6], data[7]]) as u64;
            let h = u16::from_le_bytes([data[8], data[9]]) as u64;
            return Self::validate_dimensions(w, h, max_source_pixels, max_pixel_side);
        }

        // 4. BMP: 42 4D ('BM')
        if data.len() >= 26 && data[0] == 0x42 && data[1] == 0x4D {
            let w_raw = i32::from_le_bytes([data[18], data[19], data[20], data[21]]) as i64;
            let h_raw = i32::from_le_bytes([data[22], data[23], data[24], data[25]]) as i64;
            let w = w_raw.unsigned_abs();
            let h = h_raw.unsigned_abs();
            return Self::validate_dimensions(w, h, max_source_pixels, max_pixel_side);
        }

        // 5. WebP: RIFF....WEBP
        if data.len() >= 30 && &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP" {
            if &data[12..16] == b"VP8 " && data.len() >= 30 {
                let w = (data[26] as u64 | ((data[27] as u64) << 8)) & 0x3FFF;
                let h = (data[28] as u64 | ((data[29] as u64) << 8)) & 0x3FFF;
                return Self::validate_dimensions(w, h, max_source_pixels, max_pixel_side);
            } else if &data[12..16] == b"VP8L" && data.len() >= 25 && data[20] == 0x2F {
                let b1 = data[21] as u64;
                let b2 = data[22] as u64;
                let b3 = data[23] as u64;
                let b4 = data[24] as u64;
                let w = 1 + (b1 | ((b2 & 0x3F) << 8));
                let h = 1 + ((b2 >> 6) | (b3 << 2) | ((b4 & 0xF) << 10));
                return Self::validate_dimensions(w, h, max_source_pixels, max_pixel_side);
            } else if &data[12..16] == b"VP8X" && data.len() >= 30 {
                let w =
                    1 + (data[24] as u64 | ((data[25] as u64) << 8) | ((data[26] as u64) << 16));
                let h =
                    1 + (data[27] as u64 | ((data[28] as u64) << 8) | ((data[29] as u64) << 16));
                return Self::validate_dimensions(w, h, max_source_pixels, max_pixel_side);
            }
        }

        // Unrecognized format
        Err(ResourceError::Undecodable)
    }

    fn is_svg_or_xml(mut data: &[u8]) -> bool {
        // Strip UTF-8 BOM if present
        if data.starts_with(&[0xEF, 0xBB, 0xBF]) {
            data = &data[3..];
        }

        // Skip leading whitespace
        let mut i = 0;
        while i < data.len() && matches!(data[i], b' ' | b'\t' | b'\r' | b'\n') {
            i += 1;
        }

        let slice = &data[i..];
        slice.starts_with(b"<?xml")
            || slice.starts_with(b"<svg")
            || slice.starts_with(b"<!DOCTYPE svg")
            || slice.starts_with(b"<!doctype svg")
    }

    fn validate_dimensions(
        w: u64,
        h: u64,
        max_source_pixels: u64,
        max_pixel_side: u32,
    ) -> Result<(usize, usize), ResourceError> {
        if w == 0 || h == 0 {
            return Err(ResourceError::Undecodable);
        }

        let pixels = w.saturating_mul(h);
        if pixels > max_source_pixels || w > max_pixel_side as u64 || h > max_pixel_side as u64 {
            return Err(ResourceError::TooLarge);
        }

        Ok((w as usize, h as usize))
    }
}

/// Read-only access to images in the folder of one granted document (LNX-001E).
/// Prevents folder escapes, directory traversal, non-regular files, and SVG vectors.
#[derive(Clone, Debug)]
pub struct DocumentResources {
    root: Option<PathBuf>,
    budget: ImageBudget,
}

impl DocumentResources {
    pub fn new(folder: Option<PathBuf>) -> Self {
        let root = folder.and_then(|f| std::fs::canonicalize(f).ok());
        Self {
            root,
            budget: ImageBudget::default(),
        }
    }

    pub fn with_budget(folder: Option<PathBuf>, budget: ImageBudget) -> Self {
        let root = folder.and_then(|f| std::fs::canonicalize(f).ok());
        Self { root, budget }
    }

    pub fn budget(&self) -> &ImageBudget {
        &self.budget
    }

    /// Read one image named by the core's decoded relative `path`, at most `limit` bytes.
    /// Call off the main thread.
    pub fn read_image(
        &self,
        path: &str,
        limit: usize,
    ) -> Result<(Vec<u8>, usize, usize), ResourceError> {
        if path.is_empty() || path.starts_with('/') || path.contains('\0') {
            return Err(ResourceError::OutsideFolder);
        }

        let root = self.root.as_ref().ok_or(ResourceError::OutsideFolder)?;
        let candidate = Self::normalize_lexical(root, path).ok_or(ResourceError::OutsideFolder)?;

        let resolved = std::fs::canonicalize(&candidate).map_err(Self::map_io_error)?;
        if !resolved.starts_with(root) || resolved == *root {
            return Err(ResourceError::OutsideFolder);
        }

        // File extension validation: reject SVG vectors, ensure raster
        let ext = resolved
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        if ext == "svg" {
            return Err(ResourceError::UnsupportedType);
        }
        if !matches!(
            ext.as_str(),
            "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp"
        ) {
            return Err(ResourceError::UnsupportedType);
        }

        // Linux x86-64 non-blocking open
        #[cfg(not(target_arch = "x86_64"))]
        compile_error!("Verify Linux O_NONBLOCK/O_NOCTTY flags for this architecture");
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(0x800 | 0x100)
            .open(&resolved)
            .map_err(Self::map_io_error)?;

        let meta = file.metadata().map_err(Self::map_io_error)?;
        if !meta.is_file() {
            return Err(ResourceError::NotRegularFile);
        }
        if meta.len() > limit as u64 {
            return Err(ResourceError::TooLarge);
        }

        let mut data = Vec::with_capacity(meta.len() as usize);
        file.take(limit as u64 + 1)
            .read_to_end(&mut data)
            .map_err(Self::map_io_error)?;

        if data.len() > limit {
            return Err(ResourceError::TooLarge);
        }

        let (width, height) = ImageFormatInspector::inspect(
            &data,
            self.budget.max_source_pixels,
            self.budget.max_pixel_side,
        )?;

        Ok((data, width, height))
    }

    fn normalize_lexical(root: &Path, relative: &str) -> Option<PathBuf> {
        let path = PathBuf::from(relative);
        let mut components = Vec::new();
        for comp in path.components() {
            match comp {
                std::path::Component::Normal(c) => components.push(c),
                std::path::Component::CurDir => {}
                std::path::Component::ParentDir => {
                    components.pop()?;
                }
                _ => return None,
            }
        }
        let mut result = root.to_path_buf();
        for c in components {
            result.push(c);
        }
        Some(result)
    }

    fn map_io_error(error: std::io::Error) -> ResourceError {
        match error.raw_os_error() {
            Some(2 | 20) => ResourceError::NotFound, // ENOENT / ENOTDIR
            Some(1 | 13) => ResourceError::PermissionDenied, // EPERM / EACCES
            Some(40) => ResourceError::OutsideFolder, // ELOOP
            Some(21) => ResourceError::NotRegularFile, // EISDIR
            _ => ResourceError::Undecodable,
        }
    }
}
