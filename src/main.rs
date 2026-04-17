use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use kindle_to_markdown::{
    OutputLayout, convert_to_markdown, copy_kindle_clippings, default_pull_destination,
    find_kindle_clippings_path, parse_kindle_clippings, raw_destination_for_output,
    resolve_output_target, write_markdown_output,
};
use std::fs;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "kindle-to-markdown")]
#[command(about = "Convert Kindle highlights and notes from TXT to Markdown format")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    #[arg(short, long)]
    input: Option<PathBuf>,

    #[arg(short, long)]
    output: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Command {
    Pull(PullArgs),
    Export(ExportArgs),
}

#[derive(Args)]
struct PullArgs {
    #[arg(long)]
    source: Option<PathBuf>,

    #[arg(long, default_value_os_t = default_pull_destination())]
    dest: PathBuf,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum LayoutArg {
    Single,
    ByBook,
}

#[derive(Args)]
struct ExportArgs {
    #[arg(long)]
    input: Option<PathBuf>,

    #[arg(long, default_value_t = false)]
    save_raw: bool,

    #[arg(short = 'o', long)]
    output: Option<PathBuf>,

    #[arg(long, value_enum, default_value_t = LayoutArg::Single)]
    layout: LayoutArg,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Command::Pull(args)) => pull_clippings(args),
        Some(Command::Export(args)) => export_clippings(args),
        None => convert_command(cli.input, cli.output),
    }
}

fn convert_command(input: Option<PathBuf>, output: Option<PathBuf>) -> Result<()> {
    let input = input.context("missing required argument `--input`; or use `pull` to copy My Clippings.txt from a connected Kindle")?;
    let input_content = fs::read_to_string(&input)?;
    let entries = parse_kindle_clippings(&input_content)?;
    let markdown = convert_to_markdown(&entries);

    match output {
        Some(output_path) => {
            fs::write(&output_path, markdown)?;
            println!(
                "Converted {} entries to {}",
                entries.len(),
                output_path.display()
            );
        }
        None => {
            println!("{}", markdown);
        }
    }

    Ok(())
}

fn pull_clippings(args: PullArgs) -> Result<()> {
    let source = copy_kindle_clippings(args.source.as_deref(), &args.dest)?;
    println!(
        "Copied Kindle clippings from {} to {}",
        source.display(),
        args.dest.display()
    );

    Ok(())
}

fn export_clippings(args: ExportArgs) -> Result<()> {
    let layout = map_layout(args.layout);
    let output_target = resolve_output_target(layout, args.output.as_deref());

    let input_path = match args.input {
        Some(path) => path,
        None => find_kindle_clippings_path()?,
    };

    if args.save_raw {
        let raw_destination = raw_destination_for_output(&input_path, &output_target);
        if raw_destination != input_path {
            copy_kindle_clippings(Some(&input_path), &raw_destination)?;
        }
    }

    let input_content = fs::read_to_string(&input_path).with_context(|| {
        format!(
            "failed to read clippings input from {}",
            input_path.display()
        )
    })?;
    let entries = parse_kindle_clippings(&input_content)?;
    let written = write_markdown_output(&entries, &output_target, layout)?;

    let output_label = match &output_target {
        kindle_to_markdown::OutputTarget::File(path) => path.display().to_string(),
        kindle_to_markdown::OutputTarget::Directory(path) => path.display().to_string(),
    };

    println!(
        "Exported {} entries into {} file(s) under {}",
        entries.len(),
        written.len(),
        output_label
    );

    for path in written {
        println!("{}", path.display());
    }

    if args.save_raw {
        let raw_path = raw_destination_for_output(&input_path, &output_target);
        println!("Saved raw clippings to {}", raw_path.display());
    }

    Ok(())
}

fn map_layout(layout: LayoutArg) -> OutputLayout {
    match layout {
        LayoutArg::Single => OutputLayout::SingleFile,
        LayoutArg::ByBook => OutputLayout::ByBook,
    }
}
