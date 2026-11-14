use std::sync::atomic::{AtomicU64, Ordering};

use crate::ffi::{self, md_anchor_resolution, size_of};
use crate::runtime::{CoreError, checked, submit};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnchorQuality {
    Lost = 0,
    Exact = 1,
    Relocated = 2,
    Nearest = 3,
}

impl AnchorQuality {
    pub fn from_raw(raw: u32) -> Option<Self> {
        match raw {
            ffi::MD_ANCHOR_LOST => Some(Self::Lost),
            ffi::MD_ANCHOR_EXACT => Some(Self::Exact),
            ffi::MD_ANCHOR_RELOCATED => Some(Self::Relocated),
            ffi::MD_ANCHOR_NEAREST => Some(Self::Nearest),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CoreAnchorResolution {
    pub quality: AnchorQuality,
    pub node: u32,
    pub source_offset: u64,
}

#[derive(Debug)]
pub struct CoreSnapshot {
    handle: AtomicU64,
}

impl CoreSnapshot {
    pub fn new(handle: u64) -> Self {
        Self {
            handle: AtomicU64::new(handle),
        }
    }

    pub fn handle(&self) -> u64 {
        self.handle.load(Ordering::SeqCst)
    }

    pub fn anchor_at(&self, source_offset: u64) -> Result<Vec<u8>, CoreError> {
        let handle = self.handle();
        if handle == 0 {
            return Err(CoreError::InvalidSnapshot);
        }
        let mut buffer = vec![0u8; ffi::MD_ANCHOR_MAX_BYTES as usize];
        let mut out_len = 0usize;
        let status = unsafe {
            ffi::md_snapshot_anchor_at(
                handle,
                source_offset,
                buffer.as_mut_ptr(),
                buffer.len(),
                &mut out_len,
            )
        };
        if status == ffi::MD_ERR_NO_BLOCKS {
            return Err(CoreError::NoBlocks);
        }
        checked(status)?;
        buffer.truncate(out_len.min(buffer.len()));
        Ok(buffer)
    }

    pub fn anchor_for_node(&self, node: u32, offset: u64) -> Result<Vec<u8>, CoreError> {
        let handle = self.handle();
        if handle == 0 {
            return Err(CoreError::InvalidSnapshot);
        }
        let mut buffer = vec![0u8; ffi::MD_ANCHOR_MAX_BYTES as usize];
        let mut out_len = 0usize;
        let status = unsafe {
            ffi::md_snapshot_anchor_for_node(
                handle,
                node,
                offset,
                buffer.as_mut_ptr(),
                buffer.len(),
                &mut out_len,
            )
        };
        if status == ffi::MD_ERR_NO_BLOCKS {
            return Err(CoreError::NoBlocks);
        }
        checked(status)?;
        buffer.truncate(out_len.min(buffer.len()));
        Ok(buffer)
    }

    pub fn resolve_anchor(&self, anchor: &[u8]) -> Result<Option<CoreAnchorResolution>, CoreError> {
        let handle = self.handle();
        if handle == 0 {
            return Err(CoreError::InvalidSnapshot);
        }
        let mut res = md_anchor_resolution {
            struct_size: size_of::<md_anchor_resolution>(),
            ..Default::default()
        };
        let status = unsafe {
            ffi::md_snapshot_resolve_anchor(handle, anchor.as_ptr(), anchor.len(), &mut res)
        };
        if status == ffi::MD_ERR_NO_BLOCKS {
            return Ok(None);
        }
        checked(status)?;
        let Some(quality) = AnchorQuality::from_raw(res.quality) else {
            return Ok(None);
        };
        if quality == AnchorQuality::Lost {
            return Ok(None);
        }
        Ok(Some(CoreAnchorResolution {
            quality,
            node: res.node,
            source_offset: res.source_offset,
        }))
    }
}

impl Drop for CoreSnapshot {
    fn drop(&mut self) {
        let h = self.handle.swap(0, Ordering::SeqCst);
        if h != 0 {
            submit(move || unsafe {
                ffi::md_snapshot_release(h);
            });
        }
    }
}
