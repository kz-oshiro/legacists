use crate::{AnalysisOptions, Cancellation, CoreError, Skip};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::Read,
    path::{Component, Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InputError {
    Boundary,
    Missing,
    Access,
    Changed,
    Unknown,
    Cancelled,
}

#[derive(Default)]
pub struct Listing {
    pub paths: Vec<String>,
    pub skipped: Vec<Skip>,
}

pub trait InputProvider: Send + Sync {
    fn enumerate(
        &self,
        options: &AnalysisOptions,
        cancel: &Cancellation,
    ) -> Result<Listing, InputError>;
    fn read(&self, path: &str, cancel: &Cancellation) -> Result<Vec<u8>, InputError>;
}

pub fn relative_path(path: &str) -> bool {
    !path.contains(['\\', ':', '\0'])
        && !path.starts_with('/')
        && !path.is_empty()
        && Path::new(path)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
}

pub fn excluded(path: &str, options: &AnalysisOptions) -> bool {
    options.exclusions.iter().any(|e| {
        if e.contains('/') {
            path == e || path.starts_with(&format!("{e}/"))
        } else {
            path.split('/').any(|part| part.eq_ignore_ascii_case(e))
        }
    })
}

pub fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub struct FileInput {
    root: PathBuf,
}

impl FileInput {
    pub fn new(root: impl AsRef<Path>) -> Result<Self, CoreError> {
        let root = root.as_ref();
        if !root.is_dir() || has_link_component(root) {
            return Err(CoreError::InputBoundary);
        }
        Ok(Self {
            root: fs::canonicalize(root).map_err(|_| CoreError::InputBoundary)?,
        })
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
}

pub fn has_link_component(path: &Path) -> bool {
    let mut p = PathBuf::new();
    for part in path.components() {
        p.push(part);
        if fs::symlink_metadata(&p)
            .map(|m| is_link(&m))
            .unwrap_or(false)
        {
            return true;
        }
    }
    false
}

fn is_link(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

fn io_error(e: std::io::Error) -> InputError {
    match e.kind() {
        std::io::ErrorKind::NotFound => InputError::Missing,
        std::io::ErrorKind::PermissionDenied => InputError::Access,
        _ => InputError::Unknown,
    }
}

fn handle_path(file: &File) -> Result<PathBuf, InputError> {
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::GetFinalPathNameByHandleW;
        let mut buffer = vec![0u16; 32768];
        let length = unsafe {
            GetFinalPathNameByHandleW(
                file.as_raw_handle(),
                buffer.as_mut_ptr(),
                buffer.len() as u32,
                0,
            )
        };
        if length == 0 || length as usize >= buffer.len() {
            return Err(InputError::Boundary);
        }
        Ok(PathBuf::from(
            String::from_utf16(&buffer[..length as usize]).map_err(|_| InputError::Boundary)?,
        ))
    }
    #[cfg(not(windows))]
    {
        let _ = file;
        Err(InputError::Boundary)
    }
}

impl InputProvider for FileInput {
    fn enumerate(
        &self,
        options: &AnalysisOptions,
        cancel: &Cancellation,
    ) -> Result<Listing, InputError> {
        fn walk(
            root: &Path,
            at: &Path,
            options: &AnalysisOptions,
            cancel: &Cancellation,
            out: &mut Listing,
        ) -> Result<(), InputError> {
            if cancel.is_cancelled() {
                return Err(InputError::Cancelled);
            }
            let entries = match fs::read_dir(at) {
                Ok(e) => e,
                Err(e) => {
                    out.skipped.push(Skip {
                        path: at
                            .strip_prefix(root)
                            .unwrap_or(Path::new(""))
                            .to_string_lossy()
                            .replace('\\', "/"),
                        reason: format!("read:{:?}", io_error(e)),
                    });
                    return Ok(());
                }
            };
            for entry in entries {
                if cancel.is_cancelled() {
                    return Err(InputError::Cancelled);
                }
                let entry = match entry {
                    Ok(e) => e,
                    Err(_) => {
                        out.skipped.push(Skip {
                            path: "".into(),
                            reason: "read:Unknown".into(),
                        });
                        continue;
                    }
                };
                let path = entry.path();
                let relative = path
                    .strip_prefix(root)
                    .map_err(|_| InputError::Boundary)?
                    .to_string_lossy()
                    .replace('\\', "/");
                let metadata = match fs::symlink_metadata(&path) {
                    Ok(m) => m,
                    Err(_) => {
                        out.skipped.push(Skip {
                            path: relative,
                            reason: "read:Unknown".into(),
                        });
                        continue;
                    }
                };
                let reason = if is_link(&metadata) {
                    Some("link")
                } else if excluded(&relative, options) {
                    Some("excluded")
                } else if metadata.is_file() && crate::model::language(&relative).is_none() {
                    Some("extension")
                } else {
                    None
                };
                if let Some(reason) = reason {
                    out.skipped.push(Skip {
                        path: relative,
                        reason: reason.into(),
                    });
                } else if metadata.is_dir() {
                    walk(root, &path, options, cancel, out)?;
                } else if metadata.is_file() {
                    out.paths.push(relative);
                }
            }
            Ok(())
        }
        let mut listing = Listing::default();
        walk(&self.root, &self.root, options, cancel, &mut listing)?;
        listing.paths.sort();
        listing.skipped.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(listing)
    }
    fn read(&self, path: &str, cancel: &Cancellation) -> Result<Vec<u8>, InputError> {
        if !relative_path(path) {
            return Err(InputError::Boundary);
        }
        let absolute = self.root.join(path);
        if has_link_component(&absolute) {
            return Err(InputError::Boundary);
        }
        let mut file = File::open(&absolute).map_err(io_error)?;
        let before = file.metadata().map_err(io_error)?;
        if !before.is_file() || is_link(&before) {
            return Err(InputError::Boundary);
        }
        #[cfg(windows)]
        if !handle_path(&file)?.starts_with(&self.root) {
            return Err(InputError::Boundary);
        }
        #[cfg(not(windows))]
        if !fs::canonicalize(&absolute)
            .map_err(io_error)?
            .starts_with(&self.root)
        {
            return Err(InputError::Boundary);
        }
        let mut bytes = Vec::new();
        let mut chunk = [0u8; 65536];
        loop {
            if cancel.is_cancelled() {
                return Err(InputError::Cancelled);
            }
            let n = file.read(&mut chunk).map_err(io_error)?;
            if n == 0 {
                break;
            }
            bytes.extend_from_slice(&chunk[..n]);
        }
        let after = file.metadata().map_err(io_error)?;
        if before.len() != after.len()
            || before.modified().ok() != after.modified().ok()
            || after.len() != bytes.len() as u64
        {
            return Err(InputError::Changed);
        }
        Ok(bytes)
    }
}
