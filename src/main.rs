use anyhow::{Context, Result, bail};
use clap::{Parser, ValueEnum};
use kindle_to_markdown::{
    OutputLayout, OutputTarget, convert_to_markdown, copy_kindle_clippings,
    default_export_directory, find_kindle_clippings_path, parse_kindle_clippings,
    raw_destination_for_output, render_book_stats, resolve_output_target, write_markdown_output,
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

    #[arg(short, long)]
    output: Option<PathBuf>,

    #[arg(long, value_enum, default_value_t = LayoutArg::Single)]
    layout: LayoutArg,

    #[arg(long, default_value_t = false)]
    save_raw: bool,

    #[arg(long, default_value_t = false)]
    no_stats: bool,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum LayoutArg {
    Single,
    ByBook,
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
    run(cli, io::stdin().is_terminal())
}

fn run(cli: Cli, stdin_is_terminal: bool) -> Result<()> {
    let layout = map_layout(cli.layout);
    let input_source = select_input_source(stdin_is_terminal, cli.input.as_deref(), cli.discover)?;
    let destination = select_export_destination(layout, cli.output.as_deref(), cli.discover);

    if cli.save_raw && matches!(input_source, InputSource::Stdin) {
        bail!("cannot use --save-raw with stdin input");
    }

    let (entries, raw_input_path) = match input_source {
        InputSource::Stdin => {
            let input = read_stdin_to_string()?;
            (parse_kindle_clippings(&input)?, None)
        }
        InputSource::File(path) => {
            let input = fs::read_to_string(&path).with_context(|| {
                format!("failed to read clippings input from {}", path.display())
            })?;
            (parse_kindle_clippings(&input)?, Some(path))
        }
        InputSource::Discover => {
            let path = find_kindle_clippings_path()?;
            let input = fs::read_to_string(&path).with_context(|| {
                format!("failed to read clippings input from {}", path.display())
            })?;
            (parse_kindle_clippings(&input)?, Some(path))
        }
    };

    if cli.save_raw {
        let raw_input_path = raw_input_path
            .as_deref()
            .context("save_raw requires a file-backed input source")?;
        let raw_destination = raw_destination_for_destination(raw_input_path, &destination);

        if raw_destination != raw_input_path {
            copy_kindle_clippings(Some(raw_input_path), &raw_destination)?;
            println!("Saved raw clippings to {}", raw_destination.display());
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

    if !cli.no_stats {
        eprintln!("{}", render_book_stats(&entries));
    }

    Ok(())
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

#[cfg(test)]
mod tests {
    use super::{
        Cli, ExportDestination, InputSource, LayoutArg, map_layout,
        raw_destination_for_destination, select_export_destination, select_input_source,
    };
    use clap::Parser;
    use kindle_to_markdown::{OutputLayout, OutputTarget, default_export_directory};
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
    fn cli_parses_positional_input_and_discover_flag() {
        let cli = Cli::parse_from(["kindle-to-markdown", "--discover", "--layout", "by-book"]);
        assert_eq!(cli.input, None);
        assert!(cli.discover);
        assert!(matches!(cli.layout, LayoutArg::ByBook));
    }
}
