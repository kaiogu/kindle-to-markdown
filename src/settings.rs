use anyhow::{Context, Result, anyhow, bail};
use directories::ProjectDirs;
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

const QUALIFIER: &str = "com";
const ORGANIZATION: &str = "kaiogu";
const APPLICATION: &str = "kindle-to-markdown";

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct AppSettings {
    pub discover: Option<bool>,
    pub output: Option<PathBuf>,
    pub layout: Option<SettingsLayout>,
    pub sort_by: Option<SettingsSort>,
    pub dedupe: Option<bool>,
    pub copy_raw: Option<CopyRawSetting>,
    pub no_stats: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SettingsLayout {
    Single,
    ByBook,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SettingsSort {
    Book,
    Date,
    Location,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum CopyRawSetting {
    Enabled(bool),
    Path(PathBuf),
}

pub fn settings_path() -> Result<PathBuf> {
    let project_dirs = ProjectDirs::from(QUALIFIER, ORGANIZATION, APPLICATION)
        .ok_or_else(|| anyhow!("could not determine a settings directory for this platform"))?;
    Ok(project_dirs.config_dir().join("settings.toml"))
}

pub fn load_settings() -> Result<AppSettings> {
    let path = settings_path()?;
    load_settings_from_path(&path)
}

pub fn resolved_settings_path(override_path: Option<&Path>) -> Result<PathBuf> {
    match override_path {
        Some(path) => Ok(path.to_path_buf()),
        None => settings_path(),
    }
}

pub fn example_settings_toml() -> &'static str {
    r#"# kindle-to-markdown settings
#
# Remove the leading `#` to enable a setting.

# discover = true
# output = "clippings"
# layout = "by-book"
# sort-by = "location"
# dedupe = true
# copy-raw = true
# no-stats = false
"#
}

pub fn init_settings_file(path: &Path) -> Result<()> {
    if path.exists() {
        bail!("settings file already exists at {}", path.display());
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create settings directory {}", parent.display()))?;
    }

    fs::write(path, example_settings_toml())
        .with_context(|| format!("failed to write settings file {}", path.display()))?;

    Ok(())
}

pub fn load_settings_from_path(path: &Path) -> Result<AppSettings> {
    if !path.exists() {
        return Ok(AppSettings::default());
    }

    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read settings file {}", path.display()))?;
    let settings: AppSettings = toml::from_str(&content)
        .with_context(|| format!("failed to parse settings file {}", path.display()))?;
    Ok(settings)
}

#[cfg(test)]
mod tests {
    use super::{
        AppSettings, CopyRawSetting, SettingsLayout, SettingsSort, example_settings_toml,
        init_settings_file, load_settings_from_path, resolved_settings_path,
    };
    use std::fs;
    use std::path::Path;
    use tempfile::tempdir;

    #[test]
    fn returns_default_settings_when_file_is_missing() {
        let temp = tempdir().expect("temp dir should exist");
        let settings =
            load_settings_from_path(&temp.path().join("settings.toml")).expect("load should work");

        assert_eq!(settings, AppSettings::default());
    }

    #[test]
    fn parses_settings_toml() {
        let temp = tempdir().expect("temp dir should exist");
        let settings_path = temp.path().join("settings.toml");
        fs::write(
            &settings_path,
            r#"
discover = true
output = "notes"
layout = "by-book"
sort-by = "location"
dedupe = true
copy-raw = "raw/input.txt"
no-stats = true
"#,
        )
        .expect("settings file should be written");

        let settings = load_settings_from_path(&settings_path).expect("settings should parse");

        assert_eq!(settings.discover, Some(true));
        assert_eq!(settings.output, Some("notes".into()));
        assert_eq!(settings.layout, Some(SettingsLayout::ByBook));
        assert_eq!(settings.sort_by, Some(SettingsSort::Location));
        assert_eq!(settings.dedupe, Some(true));
        assert_eq!(
            settings.copy_raw,
            Some(CopyRawSetting::Path("raw/input.txt".into()))
        );
        assert_eq!(settings.no_stats, Some(true));
    }

    #[test]
    fn parses_boolean_copy_raw_setting() {
        let temp = tempdir().expect("temp dir should exist");
        let settings_path = temp.path().join("settings.toml");
        fs::write(&settings_path, "copy-raw = true\n").expect("settings file should be written");

        let settings = load_settings_from_path(&settings_path).expect("settings should parse");

        assert_eq!(settings.copy_raw, Some(CopyRawSetting::Enabled(true)));
    }

    #[test]
    fn resolves_override_settings_path() {
        let path = Path::new("local/custom-settings.toml");
        assert_eq!(
            resolved_settings_path(Some(path)).expect("path should resolve"),
            path
        );
    }

    #[test]
    fn example_settings_template_is_commented() {
        let template = example_settings_toml();

        assert!(template.contains("# layout = \"by-book\""));
        assert!(template.contains("# sort-by = \"location\""));
        assert!(template.contains("# dedupe = true"));
    }

    #[test]
    fn initializes_new_settings_file() {
        let temp = tempdir().expect("temp dir should exist");
        let path = temp.path().join("nested").join("settings.toml");

        init_settings_file(&path).expect("settings file should be created");

        let content = fs::read_to_string(path).expect("settings file should be readable");
        assert!(content.contains("# kindle-to-markdown settings"));
    }

    #[test]
    fn refuses_to_overwrite_existing_settings_file() {
        let temp = tempdir().expect("temp dir should exist");
        let path = temp.path().join("settings.toml");
        fs::write(&path, "discover = true\n").expect("settings file should be written");

        let error = init_settings_file(&path).expect_err("existing settings should fail");

        assert!(
            error
                .to_string()
                .contains("settings file already exists at")
        );
    }
}
