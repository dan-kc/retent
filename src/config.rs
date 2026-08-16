//! TOML configuration discovery and built-in < file < CLI resolution.

use std::env;
use std::fmt::{self, Write as _};
use std::fs;
use std::path::{Path, PathBuf};

use clap::ValueEnum;
use serde::Deserialize;

use crate::document::ElementType;
use crate::filter::Filter;
use crate::scheduling::SchedulerConfig;

pub const CONFIG_FILENAME: &str = ".retent.toml";
const CONFIG_VERSION: u32 = 1;

/// Where one effective setting came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    BuiltIn,
    Config(PathBuf),
    Cli,
}

impl fmt::Display for Source {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BuiltIn => formatter.write_str("built-in"),
            Self::Config(path) => write!(formatter, "config {}", path.display()),
            Self::Cli => formatter.write_str("CLI"),
        }
    }
}

/// An effective value and the layer which selected it.
#[derive(Debug, Clone)]
pub struct Setting<T> {
    pub value: T,
    pub source: Source,
}

impl<T> Setting<T> {
    fn built_in(value: T) -> Self {
        Self {
            value,
            source: Source::BuiltIn,
        }
    }

    fn config(value: T, path: &Path) -> Self {
        Self {
            value,
            source: Source::Config(path.to_path_buf()),
        }
    }

    fn cli(value: T) -> Self {
        Self {
            value,
            source: Source::Cli,
        }
    }
}

/// Type selection shared by scheduled views.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ItemType {
    All,
    Note,
    Card,
}

impl ItemType {
    pub fn element_type(self) -> Option<ElementType> {
        match self {
            Self::All => None,
            Self::Note => Some(ElementType::Note),
            Self::Card => Some(ElementType::Card),
        }
    }
}

impl fmt::Display for ItemType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::All => "all",
            Self::Note => "note",
            Self::Card => "card",
        })
    }
}

/// Output representation shared by scheduled views.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OutputFormat {
    Table,
    Tsv,
    Paths,
    Json,
}

impl fmt::Display for OutputFormat {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Table => "table",
            Self::Tsv => "tsv",
            Self::Paths => "paths",
            Self::Json => "json",
        })
    }
}

/// Global options needed before configuration can be discovered.
#[derive(Debug, Clone, Default)]
pub struct LoadOptions {
    pub vault: Option<PathBuf>,
    pub config: Option<PathBuf>,
    pub no_config: bool,
}

/// Global scheduling values explicitly supplied on the command line.
#[derive(Debug, Clone, Copy, Default)]
pub struct SchedulerOverrides {
    pub card_retention: Option<f32>,
    pub note_max_interval: Option<u32>,
    pub note_exposure_half_life: Option<f64>,
    pub note_pass_multiplier: Option<f64>,
}

/// View values explicitly supplied on the command line.
#[derive(Debug, Clone, Default)]
pub struct ViewOverrides {
    /// `None` inherits; `Some(None)` removes a configured limit.
    pub limit: Option<Option<usize>>,
    /// `None` inherits; `Some(None)` removes a configured filter.
    pub filter: Option<Option<String>>,
    pub item_type: Option<ItemType>,
    pub format: Option<OutputFormat>,
    pub wrap: Option<bool>,
}

/// Fully layered settings for one scheduled view.
#[derive(Debug, Clone)]
pub struct ViewSettings {
    pub limit: Setting<Option<usize>>,
    pub filter: Setting<Option<String>>,
    pub item_type: Setting<ItemType>,
    pub format: Setting<OutputFormat>,
    pub wrap: Setting<bool>,
}

impl ViewSettings {
    /// Apply the final CLI layer and validate the resulting filter and limit.
    pub fn with_overrides(&self, overrides: ViewOverrides) -> Result<Self, String> {
        let mut resolved = self.clone();
        if let Some(limit) = overrides.limit {
            resolved.limit = Setting::cli(limit);
        }
        if let Some(filter) = overrides.filter {
            resolved.filter = Setting::cli(filter);
        }
        if let Some(item_type) = overrides.item_type {
            resolved.item_type = Setting::cli(item_type);
        }
        if let Some(format) = overrides.format {
            resolved.format = Setting::cli(format);
        }
        if let Some(wrap) = overrides.wrap {
            resolved.wrap = Setting::cli(wrap);
        }
        validate_view(&resolved, "CLI")?;
        Ok(resolved)
    }

    /// Parse the selected filter after precedence resolution.
    pub fn parsed_filter(&self) -> Result<Option<Filter>, String> {
        self.filter
            .value
            .as_deref()
            .map(|value| {
                value
                    .parse()
                    .map_err(|error| format!("filter from {}: {error}", self.filter.source))
            })
            .transpose()
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedScheduling {
    pub card_retention: Setting<f32>,
    pub note_max_interval: Setting<u32>,
    pub note_exposure_half_life: Setting<f64>,
    pub note_pass_multiplier: Setting<f64>,
}

impl ResolvedScheduling {
    pub fn scheduler(&self) -> SchedulerConfig {
        SchedulerConfig {
            card: crate::scheduling::card::CardSchedulerConfig {
                desired_retention: self.card_retention.value,
            },
            note: crate::scheduling::note::NoteSchedulerConfig {
                maximum_interval_days: self.note_max_interval.value,
                exposure_half_life_days: self.note_exposure_half_life.value,
                pass_multiplier: self.note_pass_multiplier.value,
            },
        }
    }
}

/// The effective configuration for the current invocation.
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub config_path: Option<PathBuf>,
    pub vault: Setting<PathBuf>,
    pub scheduling: ResolvedScheduling,
    pub queue: ViewSettings,
    pub list: ViewSettings,
}

impl RuntimeConfig {
    /// Human-readable effective values with their source layer.
    pub fn show(&self) -> String {
        let mut output = String::new();
        match &self.config_path {
            Some(path) => {
                let _ = writeln!(output, "config.file = {:?}", path.display().to_string());
            }
            None => output.push_str("config.file = none\n"),
        }
        write_setting(&mut output, "vault", &self.vault, |value| {
            format!("{:?}", value.display().to_string())
        });
        write_setting(
            &mut output,
            "scheduling.card.desired-retention",
            &self.scheduling.card_retention,
            ToString::to_string,
        );
        write_setting(
            &mut output,
            "scheduling.note.maximum-interval-days",
            &self.scheduling.note_max_interval,
            ToString::to_string,
        );
        write_setting(
            &mut output,
            "scheduling.note.exposure-half-life-days",
            &self.scheduling.note_exposure_half_life,
            ToString::to_string,
        );
        write_setting(
            &mut output,
            "scheduling.note.pass-multiplier",
            &self.scheduling.note_pass_multiplier,
            ToString::to_string,
        );
        write_view(&mut output, "queue", &self.queue);
        write_view(&mut output, "list", &self.list);
        output
    }
}

/// Discover, load, layer, and validate configuration for one invocation.
pub fn resolve(load: LoadOptions, overrides: SchedulerOverrides) -> Result<RuntimeConfig, String> {
    if load.no_config && load.config.is_some() {
        return Err("--config cannot be used with --no-config".to_owned());
    }

    let current = env::current_dir().map_err(|error| format!("current directory: {error}"))?;
    let cli_vault = load.vault.map(|path| absolute_from(&current, path));
    let config_path = if load.no_config {
        None
    } else if let Some(path) = load.config {
        Some(canonical_config(&absolute_from(&current, path))?)
    } else {
        discover(cli_vault.as_deref().unwrap_or(&current))
            .map(|path| canonical_config(&path))
            .transpose()?
    };

    let file = config_path.as_deref().map(read_config).transpose()?;
    let defaults = SchedulerConfig::default();
    let mut resolved = RuntimeConfig {
        config_path: config_path.clone(),
        vault: Setting::built_in(current),
        scheduling: ResolvedScheduling {
            card_retention: Setting::built_in(defaults.card.desired_retention),
            note_max_interval: Setting::built_in(defaults.note.maximum_interval_days),
            note_exposure_half_life: Setting::built_in(defaults.note.exposure_half_life_days),
            note_pass_multiplier: Setting::built_in(defaults.note.pass_multiplier),
        },
        queue: default_queue(),
        list: default_list(),
    };

    if let (Some(path), Some(file)) = (config_path.as_deref(), file) {
        apply_file(&mut resolved, file, path)?;
        resolved.vault = Setting::config(
            path.parent()
                .ok_or_else(|| format!("{}: config has no parent directory", path.display()))?
                .to_path_buf(),
            path,
        );
    }
    if let Some(vault) = cli_vault {
        resolved.vault = Setting::cli(vault);
    }
    apply_scheduler_overrides(&mut resolved.scheduling, overrides);
    validate_scheduling(&resolved.scheduling)?;
    validate_view(&resolved.queue, "queue")?;
    validate_view(&resolved.list, "list")?;
    Ok(resolved)
}

fn default_queue() -> ViewSettings {
    ViewSettings {
        limit: Setting::built_in(Some(5)),
        filter: Setting::built_in(None),
        item_type: Setting::built_in(ItemType::All),
        format: Setting::built_in(OutputFormat::Table),
        wrap: Setting::built_in(false),
    }
}

fn default_list() -> ViewSettings {
    ViewSettings {
        limit: Setting::built_in(None),
        filter: Setting::built_in(None),
        item_type: Setting::built_in(ItemType::All),
        format: Setting::built_in(OutputFormat::Table),
        wrap: Setting::built_in(false),
    }
}

fn apply_scheduler_overrides(scheduling: &mut ResolvedScheduling, overrides: SchedulerOverrides) {
    if let Some(value) = overrides.card_retention {
        scheduling.card_retention = Setting::cli(value);
    }
    if let Some(value) = overrides.note_max_interval {
        scheduling.note_max_interval = Setting::cli(value);
    }
    if let Some(value) = overrides.note_exposure_half_life {
        scheduling.note_exposure_half_life = Setting::cli(value);
    }
    if let Some(value) = overrides.note_pass_multiplier {
        scheduling.note_pass_multiplier = Setting::cli(value);
    }
}

fn validate_scheduling(settings: &ResolvedScheduling) -> Result<(), String> {
    let retention = settings.card_retention.value;
    if !retention.is_finite() || !(0.0..=1.0).contains(&retention) || retention == 0.0 {
        return Err(format!(
            "card desired retention from {} must be finite and in (0, 1]",
            settings.card_retention.source
        ));
    }
    if settings.note_max_interval.value == 0 {
        return Err(format!(
            "note maximum interval from {} must be at least 1 day",
            settings.note_max_interval.source
        ));
    }
    let half_life = settings.note_exposure_half_life.value;
    if !half_life.is_finite() || half_life <= 0.0 {
        return Err(format!(
            "note exposure half-life from {} must be finite and greater than zero",
            settings.note_exposure_half_life.source
        ));
    }
    let multiplier = settings.note_pass_multiplier.value;
    if !multiplier.is_finite() || multiplier < 1.0 {
        return Err(format!(
            "note pass multiplier from {} must be finite and at least 1",
            settings.note_pass_multiplier.source
        ));
    }
    Ok(())
}

fn validate_view(view: &ViewSettings, name: &str) -> Result<(), String> {
    if view.limit.value == Some(0) {
        return Err(format!(
            "{name} limit from {} must be at least 1 or \"none\"",
            view.limit.source
        ));
    }
    if let Some(filter) = &view.filter.value {
        filter
            .parse::<Filter>()
            .map_err(|error| format!("{name} filter from {}: {error}", view.filter.source))?;
    }
    if view.format.value != OutputFormat::Table && view.wrap.value {
        return Err(format!(
            "{name} wrap from {} can only be used with table output",
            view.wrap.source
        ));
    }
    Ok(())
}

fn absolute_from(base: &Path, path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        base.join(path)
    }
}

fn canonical_config(path: &Path) -> Result<PathBuf, String> {
    fs::canonicalize(path).map_err(|error| format!("{}: {error}", path.display()))
}

fn discover(start: &Path) -> Option<PathBuf> {
    let mut directory = start.to_path_buf();
    loop {
        let candidate = directory.join(CONFIG_FILENAME);
        if candidate.is_file() {
            return Some(candidate);
        }
        if !directory.pop() {
            return None;
        }
    }
}

fn read_config(path: &Path) -> Result<FileConfig, String> {
    let source =
        fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let config: FileConfig =
        toml::from_str(&source).map_err(|error| format!("{}: {error}", path.display()))?;
    if config.version != CONFIG_VERSION {
        return Err(format!(
            "{}: unsupported config version {}; expected {CONFIG_VERSION}",
            path.display(),
            config.version
        ));
    }
    Ok(config)
}

fn apply_file(resolved: &mut RuntimeConfig, file: FileConfig, path: &Path) -> Result<(), String> {
    if let Some(value) = file.scheduling.card.desired_retention {
        resolved.scheduling.card_retention = Setting::config(value, path);
    }
    if let Some(value) = file.scheduling.note.maximum_interval_days {
        resolved.scheduling.note_max_interval = Setting::config(value, path);
    }
    if let Some(value) = file.scheduling.note.exposure_half_life_days {
        resolved.scheduling.note_exposure_half_life = Setting::config(value, path);
    }
    if let Some(value) = file.scheduling.note.pass_multiplier {
        resolved.scheduling.note_pass_multiplier = Setting::config(value, path);
    }
    apply_file_view(&mut resolved.queue, file.queue, path, "queue")?;
    apply_file_view(&mut resolved.list, file.list, path, "list")?;
    Ok(())
}

fn apply_file_view(
    target: &mut ViewSettings,
    file: FileView,
    path: &Path,
    name: &str,
) -> Result<(), String> {
    if let Some(value) = file.limit {
        target.limit = Setting::config(value.resolve(path, name)?, path);
    }
    if let Some(value) = file.filter {
        target.filter = Setting::config(Some(value), path);
    }
    if let Some(value) = file.item_type {
        target.item_type = Setting::config(value, path);
    }
    if let Some(value) = file.format {
        target.format = Setting::config(value, path);
    }
    if let Some(value) = file.wrap {
        target.wrap = Setting::config(value, path);
    }
    Ok(())
}

fn write_setting<T>(
    output: &mut String,
    name: &str,
    setting: &Setting<T>,
    render: impl FnOnce(&T) -> String,
) {
    let _ = writeln!(
        output,
        "{name} = {} ({})",
        render(&setting.value),
        setting.source
    );
}

fn write_view(output: &mut String, name: &str, view: &ViewSettings) {
    write_setting(output, &format!("{name}.limit"), &view.limit, |value| {
        value.map_or_else(|| "none".to_owned(), |limit| limit.to_string())
    });
    write_setting(output, &format!("{name}.filter"), &view.filter, |value| {
        value
            .as_ref()
            .map_or_else(|| "none".to_owned(), |filter| format!("{filter:?}"))
    });
    write_setting(
        output,
        &format!("{name}.type"),
        &view.item_type,
        ToString::to_string,
    );
    write_setting(
        output,
        &format!("{name}.format"),
        &view.format,
        ToString::to_string,
    );
    write_setting(output, &format!("{name}.wrap"), &view.wrap, |value| {
        value.to_string()
    });
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct FileConfig {
    version: u32,
    #[serde(default)]
    scheduling: FileScheduling,
    #[serde(default)]
    queue: FileView,
    #[serde(default)]
    list: FileView,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
struct FileScheduling {
    card: FileCardScheduling,
    note: FileNoteScheduling,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
struct FileCardScheduling {
    desired_retention: Option<f32>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
struct FileNoteScheduling {
    maximum_interval_days: Option<u32>,
    exposure_half_life_days: Option<f64>,
    pass_multiplier: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
struct FileView {
    limit: Option<FileLimit>,
    filter: Option<String>,
    #[serde(rename = "type")]
    item_type: Option<ItemType>,
    format: Option<OutputFormat>,
    wrap: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum FileLimit {
    Count(usize),
    Name(String),
}

impl FileLimit {
    fn resolve(self, path: &Path, view: &str) -> Result<Option<usize>, String> {
        match self {
            Self::Count(0) => Err(format!(
                "{}: {view}.limit must be at least 1 or \"none\"",
                path.display()
            )),
            Self::Count(value) => Ok(Some(value)),
            Self::Name(value) if value == "none" => Ok(None),
            Self::Name(value) => Err(format!(
                "{}: {view}.limit string must be \"none\", got {value:?}",
                path.display()
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn strict_toml_config_applies_values() {
        let directory = tempdir().unwrap();
        let path = directory.path().join(CONFIG_FILENAME);
        fs::write(
            &path,
            concat!(
                "version = 1\n",
                "[scheduling.card]\n",
                "desired-retention = 0.9\n",
                "[queue]\n",
                "limit = \"none\"\n",
                "type = \"card\"\n",
                "format = \"json\"\n",
            ),
        )
        .unwrap();

        let resolved = resolve(
            LoadOptions {
                vault: Some(directory.path().to_path_buf()),
                ..LoadOptions::default()
            },
            SchedulerOverrides::default(),
        )
        .unwrap();
        assert_eq!(resolved.scheduling.card_retention.value, 0.9);
        assert_eq!(resolved.queue.limit.value, None);
        assert_eq!(resolved.queue.item_type.value, ItemType::Card);
        assert_eq!(resolved.queue.format.value, OutputFormat::Json);
    }

    #[test]
    fn cli_overrides_can_clear_file_values() {
        let base = ViewSettings {
            limit: Setting::built_in(Some(5)),
            filter: Setting::built_in(Some("priority = 5".to_owned())),
            item_type: Setting::built_in(ItemType::Card),
            format: Setting::built_in(OutputFormat::Table),
            wrap: Setting::built_in(true),
        };
        let result = base
            .with_overrides(ViewOverrides {
                limit: Some(None),
                filter: Some(None),
                item_type: Some(ItemType::All),
                wrap: Some(false),
                ..ViewOverrides::default()
            })
            .unwrap();
        assert_eq!(result.limit.value, None);
        assert_eq!(result.filter.value, None);
        assert_eq!(result.item_type.value, ItemType::All);
        assert!(!result.wrap.value);
        assert_eq!(result.limit.source, Source::Cli);
    }
}
