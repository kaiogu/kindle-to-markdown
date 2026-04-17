use anyhow::Result;
use clap::Parser;
use kindle_to_markdown::{convert_to_markdown, parse_kindle_clippings};
use std::fs;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "kindle-to-markdown")]
#[command(about = "Convert Kindle highlights and notes from TXT to Markdown format")]
struct Args {
    #[arg(short, long)]
    input: PathBuf,

    #[arg(short, long)]
    output: Option<PathBuf>,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let input_content = fs::read_to_string(&args.input)?;
    let entries = parse_kindle_clippings(&input_content)?;
    let markdown = convert_to_markdown(&entries);

    match args.output {
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
