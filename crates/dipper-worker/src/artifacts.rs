use anyhow::{Context, Result, ensure};
use dipper_core::protocol::{ArtifactFingerprint, RuntimeProvenance, WHISPER_REVISION};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, Metadata},
    io::Read,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

#[derive(Debug)]
struct ArtifactChanged(PathBuf);

impl std::fmt::Display for ArtifactChanged {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "recognition artifact changed during preparation: {}",
            self.0.display()
        )
    }
}

impl std::error::Error for ArtifactChanged {}

#[derive(Debug, PartialEq, Eq)]
struct Stamp {
    bytes: u64,
    device: u64,
    inode: u64,
    modified_s: i64,
    modified_ns: i64,
    changed_s: i64,
    changed_ns: i64,
}

impl From<&Metadata> for Stamp {
    fn from(value: &Metadata) -> Self {
        Self {
            bytes: value.len(),
            device: value.dev(),
            inode: value.ino(),
            modified_s: value.mtime(),
            modified_ns: value.mtime_nsec(),
            changed_s: value.ctime(),
            changed_ns: value.ctime_nsec(),
        }
    }
}

pub struct ArtifactGuard {
    path: PathBuf,
    stamp: Stamp,
}

impl ArtifactGuard {
    pub fn validate(&self) -> Result<()> {
        let current = fs::metadata(&self.path).context("recheck recognition artifact")?;
        if Stamp::from(&current) != self.stamp {
            return Err(ArtifactChanged(self.path.clone()).into());
        }
        Ok(())
    }
}

pub fn hash_artifact(path: &Path) -> Result<(ArtifactFingerprint, ArtifactGuard)> {
    let mut file = File::open(path).context("open recognition artifact for hashing")?;
    let before = file.metadata()?;
    ensure!(
        before.is_file(),
        "recognition artifact is not a regular file"
    );
    let stamp = Stamp::from(&before);
    let mut hash = Sha256::new();
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    if Stamp::from(&file.metadata()?) != stamp {
        return Err(ArtifactChanged(path.to_owned()).into());
    }
    let guard = ArtifactGuard {
        path: path.to_owned(),
        stamp,
    };
    guard.validate()?;
    let modified_unix_ns = before
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .and_then(|elapsed| elapsed.as_nanos().try_into().ok());
    Ok((
        ArtifactFingerprint {
            path: path.to_owned(),
            size_bytes: Some(before.len()),
            modified_unix_ns,
            sha256: Some(format!("{:x}", hash.finalize())),
            unavailable_reason: None,
        },
        guard,
    ))
}

fn loaded_library_paths() -> Result<Vec<PathBuf>> {
    // Inspect only native artifact mappings. No recognition or GPU work is triggered.
    let maps = fs::read_to_string("/proc/self/maps").context("identify loaded native artifacts")?;
    let mut paths = Vec::new();
    for line in maps.lines() {
        let path = line
            .split_whitespace()
            .skip(5)
            .collect::<Vec<_>>()
            .join(" ");
        if !path.starts_with('/') {
            continue;
        }
        let path = PathBuf::from(path);
        if !paths.contains(&path) {
            paths.push(path);
        }
    }
    Ok(paths)
}

pub fn fingerprint_runtime(model: &Path) -> Result<(RuntimeProvenance, Vec<ArtifactGuard>)> {
    let (model, model_guard) = hash_artifact(model).context("hash model weights")?;
    let mut guards = vec![model_guard];
    let mapped = loaded_library_paths();
    let mut native_artifacts = Vec::new();
    for name in [
        "libwhisper.so",
        "libggml.so",
        "libggml-base.so",
        "libggml-cpu.so",
        "libggml-vulkan.so",
    ] {
        let paths = mapped
            .as_ref()
            .ok()
            .map(|paths| {
                paths
                    .iter()
                    .filter(|path| {
                        path.file_name()
                            .and_then(|name| name.to_str())
                            .is_some_and(|file| {
                                file == name
                                    || file
                                        .strip_prefix(name)
                                        .is_some_and(|suffix| suffix.starts_with('.'))
                            })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        match paths.as_slice() {
            [path] => match hash_artifact(path) {
                Ok((fingerprint, guard)) => {
                    native_artifacts.push(fingerprint);
                    guards.push(guard);
                }
                Err(error) if error.downcast_ref::<ArtifactChanged>().is_some() => {
                    return Err(error);
                }
                Err(error) => native_artifacts.push(ArtifactFingerprint {
                    path: (*path).clone(),
                    size_bytes: None,
                    modified_unix_ns: None,
                    sha256: None,
                    unavailable_reason: Some(error.to_string()),
                }),
            },
            _ => native_artifacts.push(ArtifactFingerprint {
                path: PathBuf::from(name),
                size_bytes: None,
                modified_unix_ns: None,
                sha256: None,
                unavailable_reason: Some(if paths.is_empty() {
                    "native mapping unavailable".into()
                } else {
                    "multiple native mappings are ambiguous".into()
                }),
            }),
        }
    }
    Ok((
        RuntimeProvenance {
            engine_version: "whisper.cpp 1.9.5, source-declared".into(),
            source_revision: WHISPER_REVISION.into(),
            model,
            native_artifacts,
        },
        guards,
    ))
}
