use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use kindle_to_markdown::{
    convert_to_markdown, copy_kindle_clippings, default_pull_destination, parse_kindle_clippings,
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
}

#[derive(Args)]
struct PullArgs {
    #[arg(long)]
    source: Option<PathBuf>,

    #[arg(long, default_value_os_t = default_pull_destination())]
    dest: PathBuf,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Command::Pull(args)) => pull_clippings(args),
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
