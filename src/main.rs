use anyhow::{Context, Result, bail};
use clap::{ArgAction, Parser, ValueEnum};
use kindle_to_markdown::{
    OutputLayout, OutputTarget, SortKey, convert_to_markdown, copy_kindle_clippings,
    default_export_directory, find_kindle_clippings_path, parse_kindle_clippings, process_entries,
    raw_destination_for_output, render_book_stats, resolve_output_target,
    settings::{
        CopyRawSetting, SettingsLayout, SettingsSort, init_settings_file, load_settings_from_path,
        resolved_settings_path,
    },
    write_markdown_output,
};
use std::fs;
use std::io::{self, IsTerminal, Read};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "kindle-to-markdown")]
#[command(about = "Convert Kindle highlights and notes from TXT to Markdown format")]
struct Cli {
    #[arg(value_name = "INPUT")]
    input: Option<PathBuf>,

    #[arg(long, default_value_t = false)]
    discover: bool,

    #[arg(long, default_value_t = false)]
    print_settings_path: bool,

    #[arg(long)]
    config: Option<PathBuf>,

    #[arg(long, default_value_t = false)]
    init_config: bool,

    #[arg(short, long)]
    output: Option<PathBuf>,

    #[arg(long, value_enum)]
    layout: Option<LayoutArg>,

    #[arg(long, value_enum)]
    sort_by: Option<SortArg>,

    #[arg(long, action = ArgAction::SetTrue, overrides_with = "no_dedupe")]
    dedupe: bool,

    #[arg(long, action = ArgAction::SetTrue, overrides_with = "dedupe")]
    no_dedupe: bool,

    #[arg(long, value_name = "PATH", num_args = 0..=1, default_missing_value = "__AUTO__")]
    copy_raw: Option<String>,

    #[arg(long, default_value_t = false)]
    no_stats: bool,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum LayoutArg {
    Single,
    ByBook,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum SortArg {
    Book,
    Date,
    Location,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum InputSource {
    Stdin,
    File(PathBuf),
    Discover,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ExportDestination {
    Stdout,
    Target(OutputTarget),
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let settings_path = resolved_settings_path(cli.config.as_deref())?;

    if cli.init_config {
        init_settings_file(&settings_path)?;
        println!("Wrote example settings to {}", settings_path.display());
        return Ok(());
    }

    if cli.print_settings_path {
        println!("{}", settings_path.display());
        return Ok(());
    }

    run(cli, io::stdin().is_terminal(), &settings_path)
}

fn run(cli: Cli, stdin_is_terminal: bool, settings_path: &Path) -> Result<()> {
    let settings = load_settings_from_path(settings_path)?;
    let layout = resolve_layout(cli.layout, settings.layout);
    let sort_key = resolve_sort_key(cli.sort_by, settings.sort_by);
    let dedupe = resolve_dedupe(cli.dedupe, cli.no_dedupe, settings.dedupe);
    let discover = cli.discover || (settings.discover.unwrap_or(false) && cli.input.is_none());
    let no_stats = cli.no_stats || settings.no_stats.unwrap_or(false);
    let input_source = select_input_source(stdin_is_terminal, cli.input.as_deref(), discover)?;
    let destination = select_export_destination(
        layout,
        cli.output.as_deref().or(settings.output.as_deref()),
        discover,
    );
    let raw_copy_mode = resolve_raw_copy_mode(cli.copy_raw.as_deref(), settings.copy_raw.as_ref())?;

    let (entries, raw_input_path, raw_stdin_content) = match input_source {
        InputSource::Stdin => {
            let input = read_stdin_to_string()?;
            let entries = parse_kindle_clippings(&input)?;
            (entries, None, Some(input))
        }
        InputSource::File(path) => {
            let input = fs::read_to_string(&path).with_context(|| {
                format!("failed to read clippings input from {}", path.display())
            })?;
            (parse_kindle_clippings(&input)?, Some(path), None)
        }
        InputSource::Discover => {
            let path = find_kindle_clippings_path()?;
            let input = fs::read_to_string(&path).with_context(|| {
                format!("failed to read clippings input from {}", path.display())
            })?;
            (parse_kindle_clippings(&input)?, Some(path), None)
        }
    };
    let entries = process_entries(entries, sort_key, dedupe);

    if let Some(raw_destination) =
        resolve_raw_copy_destination(&raw_copy_mode, raw_input_path.as_deref(), &destination)?
    {
        match raw_input_path.as_deref() {
            Some(source_path) if raw_destination != source_path => {
                copy_kindle_clippings(Some(source_path), &raw_destination)?;
                println!("Copied raw clippings to {}", raw_destination.display());
            }
            Some(_) => {}
            None => {
                let stdin_content = raw_stdin_content
                    .as_deref()
                    .context("stdin raw copy content should be present")?;
                if let Some(parent) = raw_destination.parent() {
                    fs::create_dir_all(parent).with_context(|| {
                        format!("failed to create raw output directory {}", parent.display())
                    })?;
                }
                fs::write(&raw_destination, stdin_content).with_context(|| {
                    format!(
                        "failed to write raw stdin copy to {}",
                        raw_destination.display()
                    )
                })?;
                println!("Copied raw clippings to {}", raw_destination.display());
            }
        }
    }

    match &destination {
        ExportDestination::Stdout => {
            print!("{}", convert_to_markdown(&entries));
        }
        ExportDestination::Target(target) => {
            let written = write_markdown_output(&entries, target, layout)?;
            println!(
                "Exported {} entries into {} file(s) under {}",
                entries.len(),
                written.len(),
                render_output_target(target)
            );
            for path in written {
                println!("{}", path.display());
            }
        }
    }

    if !no_stats {
        eprintln!("{}", render_book_stats(&entries));
    }

    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RawCopyMode {
    Disabled,
    Auto,
    Explicit(PathBuf),
}

fn parse_raw_copy_mode(value: Option<&str>) -> Result<RawCopyMode> {
    match value {
        None => Ok(RawCopyMode::Disabled),
        Some("__AUTO__") => Ok(RawCopyMode::Auto),
        Some(path) if path.trim().is_empty() => bail!("--copy-raw path cannot be empty"),
        Some(path) => Ok(RawCopyMode::Explicit(PathBuf::from(path))),
    }
}

fn resolve_raw_copy_mode(
    cli_value: Option<&str>,
    settings_value: Option<&CopyRawSetting>,
) -> Result<RawCopyMode> {
    if cli_value.is_some() {
        return parse_raw_copy_mode(cli_value);
    }

    Ok(match settings_value {
        Some(CopyRawSetting::Enabled(true)) => RawCopyMode::Auto,
        Some(CopyRawSetting::Enabled(false)) | None => RawCopyMode::Disabled,
        Some(CopyRawSetting::Path(path)) => RawCopyMode::Explicit(path.clone()),
    })
}

fn read_stdin_to_string() -> Result<String> {
    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .context("failed to read clippings content from stdin")?;
    Ok(input)
}

fn select_input_source(
    stdin_is_terminal: bool,
    input: Option<&Path>,
    discover: bool,
) -> Result<InputSource> {
    if input.is_some() && discover {
        bail!("cannot use an input file with --discover");
    }

    if let Some(path) = input {
        Ok(InputSource::File(path.to_path_buf()))
    } else if discover {
        Ok(InputSource::Discover)
    } else if !stdin_is_terminal {
        Ok(InputSource::Stdin)
    } else {
        bail!(
            "missing input: provide a file path, pipe clippings through stdin, or pass --discover"
        )
    }
}

fn select_export_destination(
    layout: OutputLayout,
    output: Option<&Path>,
    discover: bool,
) -> ExportDestination {
    if let Some(output) = output {
        return ExportDestination::Target(resolve_output_target(layout, Some(output)));
    }

    match layout {
        OutputLayout::SingleFile if !discover => ExportDestination::Stdout,
        _ => ExportDestination::Target(OutputTarget::Directory(default_export_directory())),
    }
}

fn raw_destination_for_destination(input_path: &Path, destination: &ExportDestination) -> PathBuf {
    match destination {
        ExportDestination::Stdout => default_export_directory().join(
            input_path
                .file_name()
                .unwrap_or_else(|| std::ffi::OsStr::new("My Clippings.txt")),
        ),
        ExportDestination::Target(target) => raw_destination_for_output(input_path, target),
    }
}

fn resolve_raw_copy_destination(
    mode: &RawCopyMode,
    input_path: Option<&Path>,
    destination: &ExportDestination,
) -> Result<Option<PathBuf>> {
    match mode {
        RawCopyMode::Disabled => Ok(None),
        RawCopyMode::Explicit(path) => Ok(Some(path.clone())),
        RawCopyMode::Auto => {
            let input_path = input_path.context(
                "--copy-raw without a path requires a file path or --discover input source",
            )?;
            Ok(Some(raw_destination_for_destination(
                input_path,
                destination,
            )))
        }
    }
}

fn render_output_target(target: &OutputTarget) -> String {
    match target {
        OutputTarget::File(path) => path.display().to_string(),
        OutputTarget::Directory(path) => path.display().to_string(),
    }
}

fn map_layout(layout: LayoutArg) -> OutputLayout {
    match layout {
        LayoutArg::Single => OutputLayout::SingleFile,
        LayoutArg::ByBook => OutputLayout::ByBook,
    }
}

fn map_sort(sort: SortArg) -> SortKey {
    match sort {
        SortArg::Book => SortKey::Book,
        SortArg::Date => SortKey::Date,
        SortArg::Location => SortKey::Location,
    }
}

fn resolve_layout(
    cli_layout: Option<LayoutArg>,
    settings_layout: Option<SettingsLayout>,
) -> OutputLayout {
    match cli_layout {
        Some(layout) => map_layout(layout),
        None => match settings_layout {
            Some(SettingsLayout::ByBook) => OutputLayout::ByBook,
            Some(SettingsLayout::Single) | None => OutputLayout::SingleFile,
        },
    }
}

fn resolve_sort_key(
    cli_sort: Option<SortArg>,
    settings_sort: Option<SettingsSort>,
) -> Option<SortKey> {
    match cli_sort {
        Some(sort) => Some(map_sort(sort)),
        None => settings_sort.map(|sort| match sort {
            SettingsSort::Book => SortKey::Book,
            SettingsSort::Date => SortKey::Date,
            SettingsSort::Location => SortKey::Location,
        }),
    }
}

fn resolve_dedupe(cli_dedupe: bool, cli_no_dedupe: bool, settings_dedupe: Option<bool>) -> bool {
    if cli_dedupe {
        true
    } else if cli_no_dedupe {
        false
    } else {
        settings_dedupe.unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Cli, ExportDestination, InputSource, LayoutArg, RawCopyMode, SortArg, map_layout, map_sort,
        parse_raw_copy_mode, raw_destination_for_destination, resolve_dedupe, resolve_layout,
        resolve_raw_copy_destination, resolve_raw_copy_mode, resolve_sort_key,
        select_export_destination, select_input_source,
    };
    use clap::Parser;
    use kindle_to_markdown::{
        OutputLayout, OutputTarget, SortKey, default_export_directory,
        settings::{CopyRawSetting, SettingsLayout, SettingsSort},
    };
    use std::path::{Path, PathBuf};

    #[test]
    fn prefers_stdin_when_present() {
        let input = select_input_source(false, None, false).expect("stdin should be selected");
        assert_eq!(input, InputSource::Stdin);
    }

    #[test]
    fn uses_positional_input_when_given() {
        let input = select_input_source(true, Some(Path::new("my-clippings.txt")), false)
            .expect("file should be selected");
        assert_eq!(input, InputSource::File(PathBuf::from("my-clippings.txt")));
    }

    #[test]
    fn uses_discover_only_when_requested() {
        let input = select_input_source(true, None, true).expect("discover should be selected");
        assert_eq!(input, InputSource::Discover);
    }

    #[test]
    fn explicit_file_wins_over_implicit_stdin() {
        let input = select_input_source(false, Some(Path::new("my-clippings.txt")), false)
            .expect("file should win over implicit stdin");
        assert_eq!(input, InputSource::File(PathBuf::from("my-clippings.txt")));
    }

    #[test]
    fn rejects_file_input_with_discover() {
        let error = select_input_source(true, Some(Path::new("my-clippings.txt")), true)
            .expect_err("file and discover should conflict");
        assert!(
            error
                .to_string()
                .contains("cannot use an input file with --discover")
        );
    }

    #[test]
    fn defaults_single_layout_to_stdout_without_discover() {
        let destination = select_export_destination(OutputLayout::SingleFile, None, false);
        assert_eq!(destination, ExportDestination::Stdout);
    }

    #[test]
    fn defaults_discover_to_clippings_directory() {
        let destination = select_export_destination(OutputLayout::SingleFile, None, true);
        assert_eq!(
            destination,
            ExportDestination::Target(OutputTarget::Directory(default_export_directory()))
        );
    }

    #[test]
    fn defaults_by_book_layout_to_clippings_directory() {
        let destination = select_export_destination(OutputLayout::ByBook, None, false);
        assert_eq!(
            destination,
            ExportDestination::Target(OutputTarget::Directory(default_export_directory()))
        );
    }

    #[test]
    fn raw_destination_for_stdout_uses_clippings_directory() {
        let destination = raw_destination_for_destination(
            Path::new("/tmp/My Clippings.txt"),
            &ExportDestination::Stdout,
        );
        assert_eq!(
            destination,
            PathBuf::from("clippings").join("My Clippings.txt")
        );
    }

    #[test]
    fn layout_mapping_matches_output_layout() {
        assert_eq!(map_layout(LayoutArg::Single), OutputLayout::SingleFile);
        assert_eq!(map_layout(LayoutArg::ByBook), OutputLayout::ByBook);
    }

    #[test]
    fn sort_mapping_matches_library_sort_keys() {
        assert_eq!(map_sort(SortArg::Book), SortKey::Book);
        assert_eq!(map_sort(SortArg::Date), SortKey::Date);
        assert_eq!(map_sort(SortArg::Location), SortKey::Location);
    }

    #[test]
    fn resolves_layout_from_settings_when_cli_is_absent() {
        assert_eq!(
            resolve_layout(None, Some(SettingsLayout::ByBook)),
            OutputLayout::ByBook
        );
    }

    #[test]
    fn resolves_sort_key_from_settings_when_cli_is_absent() {
        assert_eq!(
            resolve_sort_key(None, Some(SettingsSort::Location)),
            Some(SortKey::Location)
        );
    }

    #[test]
    fn resolves_dedupe_with_cli_override() {
        assert!(resolve_dedupe(true, false, Some(false)));
        assert!(!resolve_dedupe(false, true, Some(true)));
        assert!(resolve_dedupe(false, false, Some(true)));
        assert!(!resolve_dedupe(false, false, None));
    }

    #[test]
    fn parses_copy_raw_flag_without_value_as_auto() {
        assert_eq!(
            parse_raw_copy_mode(Some("__AUTO__")).expect("auto copy mode should parse"),
            RawCopyMode::Auto
        );
    }

    #[test]
    fn parses_copy_raw_flag_with_path() {
        assert_eq!(
            parse_raw_copy_mode(Some("raw/input.txt")).expect("explicit copy mode should parse"),
            RawCopyMode::Explicit(PathBuf::from("raw/input.txt"))
        );
    }

    #[test]
    fn resolves_auto_raw_copy_for_file_backed_input() {
        let destination = resolve_raw_copy_destination(
            &RawCopyMode::Auto,
            Some(Path::new("/tmp/My Clippings.txt")),
            &ExportDestination::Target(OutputTarget::Directory(PathBuf::from("notes"))),
        )
        .expect("auto raw destination should resolve");

        assert_eq!(destination, Some(PathBuf::from("notes/My Clippings.txt")));
    }

    #[test]
    fn rejects_auto_raw_copy_for_stdin() {
        let error =
            resolve_raw_copy_destination(&RawCopyMode::Auto, None, &ExportDestination::Stdout)
                .expect_err("stdin without explicit raw path should fail");

        assert!(
            error.to_string().contains(
                "--copy-raw without a path requires a file path or --discover input source"
            )
        );
    }

    #[test]
    fn cli_parses_positional_input_and_discover_flag() {
        let cli = Cli::parse_from([
            "kindle-to-markdown",
            "--discover",
            "--layout",
            "by-book",
            "--sort-by",
            "location",
            "--dedupe",
        ]);
        assert_eq!(cli.input, None);
        assert!(cli.discover);
        assert!(matches!(cli.layout, Some(LayoutArg::ByBook)));
        assert!(matches!(cli.sort_by, Some(SortArg::Location)));
        assert!(cli.dedupe);
    }

    #[test]
    fn cli_parses_copy_raw_with_optional_path() {
        let cli = Cli::parse_from(["kindle-to-markdown", "--copy-raw=local/raw.txt"]);
        assert_eq!(cli.copy_raw, Some("local/raw.txt".to_string()));
    }

    #[test]
    fn cli_parses_config_options() {
        let cli = Cli::parse_from([
            "kindle-to-markdown",
            "--config",
            "local/settings.toml",
            "--init-config",
        ]);

        assert_eq!(cli.config, Some(PathBuf::from("local/settings.toml")));
        assert!(cli.init_config);
    }

    #[test]
    fn resolves_raw_copy_mode_from_settings_when_cli_is_absent() {
        let mode = resolve_raw_copy_mode(None, Some(&CopyRawSetting::Enabled(true)))
            .expect("settings raw copy should resolve");
        assert_eq!(mode, RawCopyMode::Auto);
    }
}
