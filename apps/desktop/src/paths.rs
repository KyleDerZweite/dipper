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
        let executable = env::current_exe()?;
        let escaped = executable
            .to_string_lossy()
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('`', "\\`")
            .replace('$', "\\$");
        let legacy = format!(
            "[Desktop Entry]\nType=Application\nName=Dipper\nComment=Local voice dictation\nExec=\"{escaped}\"\nNoDisplay=true\nTerminal=false\nCategories=Utility;\n"
        );
        let brand_dir = applications
            .parent()
            .context("application data directory")?
            .join("dipper/branding");
        let icon = brand_dir.join("logo.png");
        let icon_name = icon
            .to_string_lossy()
            .replace('\\', "\\\\")
            .replace('\n', "\\n")
            .replace('\r', "\\r");
        let branded = format!("{legacy}Icon={icon_name}\n");
        // Upgrade only our exact previous development entry. Installed or
        // user-edited entries remain owned by their installer/user.
        let previous = if path.exists() {
            Some(fs::read_to_string(&path)?)
        } else {
            None
        };
        if previous
            .as_deref()
            .is_some_and(|content| content != legacy && content != branded)
        {
            return Ok(());
        }
        fs::create_dir_all(&brand_dir)?;
        if fs::read(&icon).ok().as_deref() != Some(include_bytes!("../../../Logo.png").as_slice()) {
            atomic_write(&icon, include_bytes!("../../../Logo.png"))?;
        }
        if previous.as_deref() != Some(&branded) {
            atomic_write(&path, branded.as_bytes())?;
        }
        // GTK's Wayland window uses the executable name as its app ID. KDE
        // resolves its titlebar/task icon through this hidden alias, while the
        // portal continues to use the full identifier above.
        let alias = applications.join("dipper.desktop");
        if !alias.exists() {
            atomic_write(&alias, branded.as_bytes())?;
        }
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

fn atomic_write(path: &std::path::Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().context("branding destination directory")?;
    let temporary = parent.join(format!(".dipper-branding-{}", uuid::Uuid::new_v4()));
    let result = (|| -> Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)?;
        file.write_all(bytes)?;
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
