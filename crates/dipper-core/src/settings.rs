use crate::protocol::RecognitionModel;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    #[serde(default)]
    pub model_profile: RecognitionModel,
    pub input_name: Option<String>,
    pub model_path: PathBuf,
    pub native_lib_dir: PathBuf,
    pub preferred_hold: String,
}

impl Settings {
    pub fn defaults(data_dir: &Path) -> Self {
        let model_path = std::env::var_os("DIPPER_MODEL_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|| data_dir.join("models/ggml-large-v3-turbo.bin"));
        let native_lib_dir = std::env::var_os("DIPPER_WHISPER_DIR")
            .map(|dir| PathBuf::from(dir).join("build/bin"))
            .unwrap_or_else(|| data_dir.join("runtime/lib"));
        Self {
            model_profile: RecognitionModel::LargeV3Turbo,
            input_name: None,
            model_path,
            native_lib_dir,
            preferred_hold: "F2".into(),
        }
    }

    pub fn load(path: &Path, data_dir: &Path) -> Result<Self> {
        match fs::read(path) {
            Ok(bytes) => {
                ensure!(bytes.len() <= 64 * 1024, "settings file exceeds size limit");
                let settings: Self = serde_json::from_slice(&bytes).context("read settings")?;
                settings.validate()?;
                Ok(settings)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(Self::defaults(data_dir))
            }
            Err(error) => Err(error).context("read settings"),
        }
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(self.model_path.is_absolute(), "model path must be absolute");
        ensure!(
            self.native_lib_dir.is_absolute(),
            "runtime library path must be absolute"
        );
        ensure!(
            !self.preferred_hold.trim().is_empty() && self.preferred_hold.len() <= 128,
            "enter a preferred Hold key"
        );
        ensure!(
            self.input_name
                .as_ref()
                .is_none_or(|name| !name.is_empty() && name.len() <= 1024),
            "invalid input name"
        );
        Ok(())
    }

    pub fn ready_for_recognition(&self) -> Result<()> {
        self.validate()?;
        ensure!(
            self.model_path.is_file(),
            "Whisper model is unavailable. Set its path in Settings."
        );
        ensure!(
            self.native_lib_dir.join("libwhisper.so").is_file(),
            "Whisper runtime is unavailable. Set its library directory in Settings."
        );
        Ok(())
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        self.validate()?;
        let parent = path.parent().context("settings parent directory")?;
        fs::create_dir_all(parent)?;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
        let temporary = parent.join(format!("settings-{}.tmp", uuid::Uuid::new_v4()));
        let result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&temporary)?;
            file.write_all(&serde_json::to_vec_pretty(self)?)?;
            file.sync_all()?;
            fs::rename(&temporary, path)?;
            fs::File::open(parent)?.sync_all()?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }
}
