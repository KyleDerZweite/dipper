use anyhow::{Context, Result, ensure};
use std::{
    env,
    fs::{self, OpenOptions},
    io::Write,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::PathBuf,
};

pub struct AppPaths {
    pub data: PathBuf,
    pub state: PathBuf,
    pub settings: PathBuf,
    pub worker: PathBuf,
    pub clipboard_helper: PathBuf,
}

impl AppPaths {
    pub fn lock_session(&self) -> Result<fs::File> {
        use fs2::FileExt;
        let runtime = PathBuf::from(
            env::var_os("XDG_RUNTIME_DIR").context("XDG_RUNTIME_DIR is unavailable")?,
        );
        ensure!(runtime.is_absolute(), "runtime directory must be absolute");
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(runtime.join("dipper-app.lock"))?;
        lock.try_lock_exclusive()
            .context("Dipper is already running in this desktop session")?;
        Ok(lock)
    }

    pub fn resolve() -> Result<Self> {
        let home = env::var_os("HOME").context("HOME is unavailable")?;
        let home = PathBuf::from(home);
        let data = env::var_os("DIPPER_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                env::var_os("XDG_DATA_HOME")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| home.join(".local/share"))
                    .join("dipper/app")
            });
        let state = env::var_os("DIPPER_STATE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                env::var_os("XDG_STATE_HOME")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| home.join(".local/state"))
                    .join("dipper/app")
            });
        ensure!(
            data.is_absolute() && state.is_absolute(),
            "application storage paths must be absolute"
        );
        let executable = env::current_exe()?;
        let bin = executable
            .parent()
            .context("application executable directory")?;
        Ok(Self {
            settings: data.join("settings.json"),
            data,
            state,
            worker: bin.join("dipper-worker"),
            clipboard_helper: bin.join("dipper-clipboard"),
        })
    }

    pub fn ensure_desktop_identity(&self) -> Result<()> {
        let home = PathBuf::from(env::var_os("HOME").context("HOME is unavailable")?);
        let applications = env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local/share"))
            .join("applications");
        fs::create_dir_all(&applications)?;
        let path = applications.join("io.github.KyleDerZweite.dipper.desktop");
        // Preserve an existing installed entry. The portal requires this exact app identity.
        if path.exists() {
            return Ok(());
        }
        let executable = env::current_exe()?;
        let escaped = executable
            .to_string_lossy()
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('`', "\\`")
            .replace('$', "\\$");
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)?;
        writeln!(
            file,
            "[Desktop Entry]\nType=Application\nName=Dipper\nComment=Local voice dictation\nExec=\"{escaped}\"\nNoDisplay=true\nTerminal=false\nCategories=Utility;"
        )?;
        file.sync_all()?;
        fs::File::open(applications)?.sync_all()?;
        Ok(())
    }

    pub fn private_directories(&self) -> Result<()> {
        for path in [&self.data, &self.state] {
            fs::create_dir_all(path)?;
            fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
        }
        Ok(())
    }
}
