use anyhow::{Context, Result, anyhow};
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
        AppSettings, CopyRawSetting, SettingsLayout, SettingsSort, load_settings_from_path,
    };
    use std::fs;
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
}
