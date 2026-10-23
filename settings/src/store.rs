//! Safe, comment-preserving edits to the installed zDesk configuration.

use std::fs::{self, File, OpenOptions, Permissions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use toml_edit::{DocumentMut, Item, Value};

pub struct Store {
    path: PathBuf,
    validator: PathBuf,
    original: String,
    document: DocumentMut,
}

impl Store {
    pub fn open() -> Result<Self, String> {
        let home = std::env::var_os("HOME").ok_or("HOME is not set")?;
        let home = PathBuf::from(home);
        Self::open_at(
            home.join(".config/zdesk/config.toml"),
            home.join(".local/bin/zdesk"),
        )
    }

    fn open_at(path: PathBuf, validator: PathBuf) -> Result<Self, String> {
        let original = fs::read_to_string(&path)
            .map_err(|error| format!("Cannot read {}: {error}", path.display()))?;
        let document = original
            .parse::<DocumentMut>()
            .map_err(|error| format!("Invalid TOML in {}: {error}", path.display()))?;
        Ok(Self {
            path,
            validator,
            original,
            document,
        })
    }

    pub fn get_f64(&self, section: &str, key: &str) -> Result<f64, String> {
        self.item(section, key)?
            .as_float()
            .ok_or_else(|| format!("{section}.{key} is not a decimal number"))
    }

    pub fn get_i64(&self, section: &str, key: &str) -> Result<i64, String> {
        self.item(section, key)?
            .as_integer()
            .ok_or_else(|| format!("{section}.{key} is not an integer"))
    }

    pub fn get_text(&self, section: &str, key: &str) -> Result<String, String> {
        self.item(section, key)?
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| format!("{section}.{key} is not text"))
    }

    pub fn set_f64(&mut self, section: &str, key: &str, value: f64) -> Result<(), String> {
        if !value.is_finite() {
            return Err(format!("{section}.{key} must be a finite number"));
        }
        self.set_value(section, key, Value::from(value))
    }

    pub fn set_i64(&mut self, section: &str, key: &str, value: i64) -> Result<(), String> {
        self.set_value(section, key, Value::from(value))
    }

    pub fn set_text(&mut self, section: &str, key: &str, value: &str) -> Result<(), String> {
        self.set_value(section, key, Value::from(value.to_owned()))
    }

    /// Begin each save attempt from the last successfully saved content.
    pub fn reset_draft(&mut self) -> Result<(), String> {
        self.document = self
            .original
            .parse::<DocumentMut>()
            .map_err(|error| format!("Cannot reopen configuration draft: {error}"))?;
        Ok(())
    }

    /// Validate a draft with the actual widget binary, then back up and atomically save it.
    /// The returned path is the backup of the previous configuration.
    pub fn save(&mut self) -> Result<PathBuf, String> {
        let current = fs::read_to_string(&self.path)
            .map_err(|error| format!("Cannot reread {}: {error}", self.path.display()))?;
        if current != self.original {
            return Err(
                "Configuration changed outside this window. Reopen settings before saving.".into(),
            );
        }
        let metadata = fs::symlink_metadata(&self.path)
            .map_err(|error| format!("Cannot inspect {}: {error}", self.path.display()))?;
        if !metadata.file_type().is_file() {
            return Err("Configuration path must be a regular file.".into());
        }

        let candidate = self.document.to_string();
        candidate
            .parse::<DocumentMut>()
            .map_err(|error| format!("Generated TOML is invalid: {error}"))?;
        let parent = self
            .path
            .parent()
            .ok_or("Configuration has no parent directory")?;
        let temp = write_unique_file(
            parent,
            ".config.toml.new",
            candidate.as_bytes(),
            &metadata.permissions(),
        )?;

        let output = Command::new(&self.validator)
            .arg("--check-config")
            .arg(&temp.path)
            .output()
            .map_err(|error| format!("Cannot run {}: {error}", self.validator.display()))?;
        if !output.status.success() {
            let detail = String::from_utf8_lossy(&output.stderr);
            let detail = detail.trim();
            return Err(if detail.is_empty() {
                format!("Configuration validation failed ({})", output.status)
            } else {
                format!("Configuration validation failed: {detail}")
            });
        }

        // A text editor might have changed the file while validation was running.
        let latest = fs::read_to_string(&self.path)
            .map_err(|error| format!("Cannot reread {}: {error}", self.path.display()))?;
        if latest != self.original {
            return Err(
                "Configuration changed outside this window. Reopen settings before saving.".into(),
            );
        }

        let backup = write_unique_file(
            parent,
            "config.toml.bak",
            self.original.as_bytes(),
            &metadata.permissions(),
        )?;
        let backup_path = backup.keep();

        fs::rename(&temp.path, &self.path)
            .map_err(|error| format!("Cannot replace {}: {error}", self.path.display()))?;
        self.original = candidate;
        File::open(parent)
            .and_then(|dir| dir.sync_all())
            .map_err(|error| {
                format!("Configuration was saved, but directory sync failed: {error}")
            })?;
        Ok(backup_path)
    }

    fn item(&self, section: &str, key: &str) -> Result<&Item, String> {
        validate_address(section, key)?;
        let mut item = self.document.as_item();
        for segment in section.split('.') {
            item = item
                .get(segment)
                .ok_or_else(|| format!("Missing section [{section}]"))?;
        }
        item.get(key)
            .ok_or_else(|| format!("Missing setting {section}.{key}"))
    }

    fn item_mut(&mut self, section: &str, key: &str) -> Result<&mut Item, String> {
        validate_address(section, key)?;
        let mut item = self.document.as_item_mut();
        for segment in section.split('.') {
            item = item
                .get_mut(segment)
                .ok_or_else(|| format!("Missing section [{section}]"))?;
        }
        item.get_mut(key)
            .ok_or_else(|| format!("Missing setting {section}.{key}"))
    }

    fn set_value(&mut self, section: &str, key: &str, new_value: Value) -> Result<(), String> {
        let value = self
            .item_mut(section, key)?
            .as_value_mut()
            .ok_or_else(|| format!("{section}.{key} is not a value"))?;
        let decor = value.decor().clone();
        *value = new_value;
        *value.decor_mut() = decor;
        Ok(())
    }
}

fn validate_address(section: &str, key: &str) -> Result<(), String> {
    if section.is_empty() || section.split('.').any(str::is_empty) || key.is_empty() {
        return Err("Invalid configuration section or key".into());
    }
    Ok(())
}

struct TemporaryFile {
    path: PathBuf,
    remove_on_drop: bool,
}

impl TemporaryFile {
    fn keep(mut self) -> PathBuf {
        self.remove_on_drop = false;
        self.path.clone()
    }
}

impl Drop for TemporaryFile {
    fn drop(&mut self) {
        if self.remove_on_drop {
            let _ = fs::remove_file(&self.path);
        }
    }
}

fn write_unique_file(
    parent: &Path,
    prefix: &str,
    contents: &[u8],
    permissions: &Permissions,
) -> Result<TemporaryFile, String> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("System clock error: {error}"))?
        .as_nanos();
    for attempt in 0..100 {
        let name = format!("{prefix}-{}-{stamp}-{attempt}", std::process::id());
        let path = parent.join(name);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
        {
            Ok(mut file) => {
                let guard = TemporaryFile {
                    path,
                    remove_on_drop: true,
                };
                file.write_all(contents)
                    .and_then(|_| file.set_permissions(permissions.clone()))
                    .and_then(|_| file.sync_all())
                    .map_err(|error| format!("Cannot write {}: {error}", guard.path.display()))?;
                return Ok(guard);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(format!("Cannot create {}: {error}", path.display())),
        }
    }
    Err(format!(
        "Cannot create a unique file in {}",
        parent.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    struct Fixture {
        dir: PathBuf,
        config: PathBuf,
        validator: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let dir = std::env::temp_dir()
                .join(format!("zdesk-store-test-{}-{stamp}", std::process::id()));
            fs::create_dir(&dir).unwrap();
            let config = dir.join("config.toml");
            fs::write(
                &config,
                "# user's comment\n[clock]\nfont_size = 180.0 # keep this\nname = 'custom'\n\n[rings.cpu]\ncenter_x = 100\n",
            )
            .unwrap();
            fs::set_permissions(&config, Permissions::from_mode(0o640)).unwrap();
            let validator = dir.join("validator");
            fs::write(
                &validator,
                "#!/bin/sh\n[ \"$1\" = '--check-config' ] || exit 2\ncase \"$(cat \"$2\")\" in *reject*) echo rejected >&2; exit 1;; esac\nexit 0\n",
            )
            .unwrap();
            fs::set_permissions(&validator, Permissions::from_mode(0o700)).unwrap();
            Self {
                dir,
                config,
                validator,
            }
        }

        fn store(&self) -> Store {
            Store::open_at(self.config.clone(), self.validator.clone()).unwrap()
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn edits_keep_comments_and_create_backup() {
        let fixture = Fixture::new();
        let mut store = fixture.store();
        assert_eq!(store.get_f64("clock", "font_size").unwrap(), 180.0);
        assert_eq!(store.get_text("clock", "name").unwrap(), "custom");
        assert_eq!(store.get_i64("rings.cpu", "center_x").unwrap(), 100);
        store.set_f64("clock", "font_size", 150.0).unwrap();
        store.set_text("clock", "name", "new name").unwrap();
        store.set_i64("rings.cpu", "center_x", 110).unwrap();
        let backup = store.save().unwrap();
        let saved = fs::read_to_string(&fixture.config).unwrap();
        assert!(saved.contains("# user's comment"));
        assert!(saved.contains("150.0 # keep this"));
        assert!(saved.contains("name = \"new name\""));
        assert!(saved.contains("center_x = 110"));
        assert!(fs::read_to_string(backup)
            .unwrap()
            .contains("font_size = 180.0"));
        assert_eq!(
            fs::metadata(&fixture.config).unwrap().permissions().mode() & 0o777,
            0o640
        );
    }

    #[test]
    fn external_edit_prevents_overwrite() {
        let fixture = Fixture::new();
        let mut store = fixture.store();
        store.set_f64("clock", "font_size", 150.0).unwrap();
        fs::write(&fixture.config, "[clock]\nfont_size = 200.0\n").unwrap();
        assert!(store.save().unwrap_err().contains("changed outside"));
        assert!(fs::read_to_string(&fixture.config)
            .unwrap()
            .contains("200.0"));
    }

    #[test]
    fn rejected_draft_does_not_replace_config() {
        let fixture = Fixture::new();
        let mut store = fixture.store();
        let original = fs::read_to_string(&fixture.config).unwrap();
        store.set_text("clock", "name", "reject").unwrap();
        assert!(store.save().unwrap_err().contains("rejected"));
        assert_eq!(fs::read_to_string(&fixture.config).unwrap(), original);
    }
}
