use anyhow::{Context, Result, anyhow, bail};
use regex::Regex;
use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KindleEntry {
    pub title: String,
    pub author: String,
    pub entry_type: String,
    pub location: String,
    pub date: String,
    pub content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostPlatform {
    Windows,
    MacOs,
    Linux,
    Wsl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputLayout {
    SingleFile,
    ByBook,
}

pub fn detect_host_platform() -> HostPlatform {
    if cfg!(windows) {
        HostPlatform::Windows
    } else if env::var_os("WSL_DISTRO_NAME").is_some() || env::var_os("WSL_INTEROP").is_some() {
        HostPlatform::Wsl
    } else if cfg!(target_os = "macos") {
        HostPlatform::MacOs
    } else {
        HostPlatform::Linux
    }
}

pub fn default_pull_destination() -> PathBuf {
    PathBuf::from("local").join("my-clippings.txt")
}

pub fn default_export_directory() -> PathBuf {
    PathBuf::from("clippings")
}

pub fn copy_kindle_clippings(source: Option<&Path>, destination: &Path) -> Result<PathBuf> {
    let source_path = match source {
        Some(path) => path.to_path_buf(),
        None => find_kindle_clippings_path()?,
    };

    if !source_path.is_file() {
        bail!(
            "Kindle clippings file not found at {}",
            source_path.display()
        );
    }

    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create destination directory {}",
                parent.display()
            )
        })?;
    }

    fs::copy(&source_path, destination).with_context(|| {
        format!(
            "failed to copy clippings from {} to {}",
            source_path.display(),
            destination.display()
        )
    })?;

    Ok(source_path)
}

pub fn find_kindle_clippings_path() -> Result<PathBuf> {
    let roots = device_roots_for_platform(detect_host_platform(), env::var_os("USER"));
    find_kindle_clippings_path_from_roots(&roots).ok_or_else(|| {
        anyhow!(
            "could not find My Clippings.txt on a connected Kindle. Try `pull --source /path/to/My Clippings.txt` if auto-detection misses your device."
        )
    })
}

fn find_kindle_clippings_path_from_roots(roots: &[PathBuf]) -> Option<PathBuf> {
    roots
        .iter()
        .flat_map(|root| clippings_paths_for_device_root(root))
        .find(|path| path.is_file())
}

fn clippings_paths_for_device_root(root: &Path) -> [PathBuf; 2] {
    [
        root.join("My Clippings.txt"),
        root.join("documents").join("My Clippings.txt"),
    ]
}

fn device_roots_for_platform(
    platform: HostPlatform,
    user: Option<std::ffi::OsString>,
) -> Vec<PathBuf> {
    let roots = match platform {
        HostPlatform::Windows => windows_drive_roots(),
        HostPlatform::Wsl => wsl_drive_roots(),
        HostPlatform::MacOs => mounted_children([PathBuf::from("/Volumes")]),
        HostPlatform::Linux => linux_mount_roots(user),
    };

    dedupe_paths(roots)
}

fn windows_drive_roots() -> Vec<PathBuf> {
    (b'A'..=b'Z')
        .map(|letter| PathBuf::from(format!("{}:\\", letter as char)))
        .collect()
}

fn wsl_drive_roots() -> Vec<PathBuf> {
    (b'a'..=b'z')
        .map(|letter| PathBuf::from(format!("/mnt/{}", letter as char)))
        .collect()
}

fn linux_mount_roots(user: Option<std::ffi::OsString>) -> Vec<PathBuf> {
    let mut parents = vec![PathBuf::from("/mnt")];

    if let Some(user) = user {
        parents.push(PathBuf::from("/run/media").join(&user));
        parents.push(PathBuf::from("/media").join(&user));
    }

    parents.push(PathBuf::from("/run/media"));
    parents.push(PathBuf::from("/media"));

    mounted_children(parents)
}

fn mounted_children(parents: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut roots = Vec::new();

    for parent in parents {
        let Ok(entries) = fs::read_dir(&parent) else {
            continue;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                roots.push(path);
            }
        }
    }

    roots
}

fn dedupe_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = BTreeSet::new();
    let mut deduped = Vec::new();

    for path in paths {
        if seen.insert(path.clone()) {
            deduped.push(path);
        }
    }

    deduped
}

pub fn parse_kindle_clippings(content: &str) -> Result<Vec<KindleEntry>> {
    let mut entries = Vec::new();
    let separator = "==========";
    let chunks: Vec<&str> = content.split(separator).collect();

    let re = Regex::new(
        r"^(.+?) \((.+?)\) - Your (Highlight|Note|Bookmark) (?:on page \d+ \| )?Location ([^|]+) \| Added on (.+)$",
    )?;

    for chunk in chunks {
        let chunk = chunk.trim();
        if chunk.is_empty() {
            continue;
        }

        let lines: Vec<&str> = chunk.lines().collect();
        if lines.is_empty() {
            continue;
        }

        let header = lines[0];
        let entry_content = lines.get(1..).unwrap_or(&[]).join("\n").trim().to_string();

        if let Some(captures) = re.captures(header) {
            let entry = KindleEntry {
                title: captures[1].to_string(),
                author: captures[2].to_string(),
                entry_type: captures[3].to_string(),
                location: captures[4].trim().to_string(),
                date: captures[5].trim().to_string(),
                content: entry_content,
            };
            entries.push(entry);
        }
    }

    Ok(entries)
}

pub fn convert_to_markdown(entries: &[KindleEntry]) -> String {
    let mut markdown = String::new();
    let mut current_book = String::new();

    for entry in entries {
        let book_title = format!("{} by {}", entry.title, entry.author);

        if book_title != current_book {
            if !current_book.is_empty() {
                markdown.push_str("\n---\n\n");
            }
            markdown.push_str(&format!("# {}\n\n", book_title));
            current_book = book_title;
        }

        match entry.entry_type.as_str() {
            "Highlight" => {
                markdown.push_str(&format!("> {}\n\n", entry.content));
                markdown.push_str(&format!(
                    "*Location: {} | Added: {}*\n\n",
                    entry.location, entry.date
                ));
            }
            "Note" => {
                markdown.push_str(&format!("**Note:** {}\n\n", entry.content));
                markdown.push_str(&format!(
                    "*Location: {} | Added: {}*\n\n",
                    entry.location, entry.date
                ));
            }
            "Bookmark" => {
                markdown.push_str(&format!("**Bookmark** at Location: {}\n\n", entry.location));
                markdown.push_str(&format!("*Added: {}*\n\n", entry.date));
            }
            _ => {}
        }
    }

    markdown
}

pub fn write_markdown_output(
    entries: &[KindleEntry],
    output_dir: &Path,
    layout: OutputLayout,
) -> Result<Vec<PathBuf>> {
    fs::create_dir_all(output_dir)
        .with_context(|| format!("failed to create output directory {}", output_dir.display()))?;

    match layout {
        OutputLayout::SingleFile => write_single_markdown_file(entries, output_dir),
        OutputLayout::ByBook => write_markdown_files_by_book(entries, output_dir),
    }
}

fn write_single_markdown_file(entries: &[KindleEntry], output_dir: &Path) -> Result<Vec<PathBuf>> {
    let destination = output_dir.join("clippings.md");
    fs::write(&destination, convert_to_markdown(entries))
        .with_context(|| format!("failed to write markdown file {}", destination.display()))?;

    Ok(vec![destination])
}

fn write_markdown_files_by_book(
    entries: &[KindleEntry],
    output_dir: &Path,
) -> Result<Vec<PathBuf>> {
    let mut written = Vec::new();
    let mut current_book = None::<(String, String)>;
    let mut current_entries = Vec::new();

    for entry in entries {
        let book = (entry.title.clone(), entry.author.clone());
        if current_book.as_ref() != Some(&book) {
            if let Some((title, author)) = current_book.take() {
                written.push(write_book_markdown_file(
                    output_dir,
                    &title,
                    &author,
                    &current_entries,
                )?);
                current_entries.clear();
            }
            current_book = Some(book);
        }

        current_entries.push(entry.clone());
    }

    if let Some((title, author)) = current_book {
        written.push(write_book_markdown_file(
            output_dir,
            &title,
            &author,
            &current_entries,
        )?);
    }

    Ok(written)
}

fn write_book_markdown_file(
    output_dir: &Path,
    title: &str,
    author: &str,
    entries: &[KindleEntry],
) -> Result<PathBuf> {
    let file_name = format!("{}.md", slugify_book_title(title, author));
    let destination = output_dir.join(file_name);
    fs::write(&destination, convert_to_markdown(entries))
        .with_context(|| format!("failed to write markdown file {}", destination.display()))?;

    Ok(destination)
}

fn slugify_book_title(title: &str, author: &str) -> String {
    let combined = format!("{} {}", title, author).to_lowercase();
    let mut slug = String::with_capacity(combined.len());
    let mut last_was_separator = false;

    for ch in combined.chars() {
        let is_word = ch.is_ascii_alphanumeric();
        if is_word {
            slug.push(ch);
            last_was_separator = false;
        } else if !last_was_separator {
            slug.push('-');
            last_was_separator = true;
        }
    }

    let trimmed = slug.trim_matches('-');
    if trimmed.is_empty() {
        "book".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        HostPlatform, OutputLayout, clippings_paths_for_device_root, convert_to_markdown,
        copy_kindle_clippings, default_export_directory, default_pull_destination,
        device_roots_for_platform, find_kindle_clippings_path_from_roots, parse_kindle_clippings,
        write_markdown_output,
    };
    use std::fs;
    use std::path::PathBuf;
    use tempfile::tempdir;

    const SAMPLE_INPUT: &str = r#"The Rust Programming Language (Steve Klabnik, Carol Nichols) - Your Highlight on page 23 | Location 234-236 | Added on Friday, August 9, 2024 12:34:56 PM

Memory safety is one of Rust's main selling points.

==========
The Rust Programming Language (Steve Klabnik, Carol Nichols) - Your Note on page 24 | Location 240-240 | Added on Friday, August 9, 2024 12:36:10 PM

Great explanation of ownership here.

==========
The Rust Programming Language (Steve Klabnik, Carol Nichols) - Your Bookmark on page 30 | Location 300-300 | Added on Friday, August 9, 2024 12:40:00 PM


==========
Broken Entry Without Metadata

This should be ignored.

==========
"#;

    #[test]
    fn parses_supported_entry_types_and_skips_invalid_chunks() {
        let entries = parse_kindle_clippings(SAMPLE_INPUT).expect("sample input should parse");

        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].entry_type, "Highlight");
        assert_eq!(entries[1].entry_type, "Note");
        assert_eq!(entries[2].entry_type, "Bookmark");
        assert_eq!(entries[0].location, "234-236");
        assert_eq!(entries[1].content, "Great explanation of ownership here.");
        assert!(entries[2].content.is_empty());
    }

    #[test]
    fn renders_grouped_markdown_with_metadata() {
        let entries = parse_kindle_clippings(SAMPLE_INPUT).expect("sample input should parse");
        let markdown = convert_to_markdown(&entries);

        assert!(
            markdown.contains("# The Rust Programming Language by Steve Klabnik, Carol Nichols")
        );
        assert!(markdown.contains("> Memory safety is one of Rust's main selling points."));
        assert!(markdown.contains("**Note:** Great explanation of ownership here."));
        assert!(markdown.contains("**Bookmark** at Location: 300-300"));
        assert!(
            markdown.contains("*Location: 234-236 | Added: Friday, August 9, 2024 12:34:56 PM*")
        );
    }

    #[test]
    fn separates_books_with_markdown_rule() {
        let input = r#"Book One (Author A) - Your Highlight on page 1 | Location 1-1 | Added on Monday, January 1, 2024 10:00:00 AM

Alpha

==========
Book Two (Author B) - Your Highlight on page 2 | Location 2-2 | Added on Monday, January 1, 2024 11:00:00 AM

Beta

==========
"#;

        let entries = parse_kindle_clippings(input).expect("input should parse");
        let markdown = convert_to_markdown(&entries);

        assert!(markdown.contains("# Book One by Author A"));
        assert!(markdown.contains("\n---\n\n# Book Two by Author B"));
    }

    #[test]
    fn builds_clippings_paths_for_root_and_documents_folder() {
        let root = PathBuf::from("/Volumes/Kindle");
        let candidates = clippings_paths_for_device_root(&root);

        assert_eq!(
            candidates[0],
            PathBuf::from("/Volumes/Kindle/My Clippings.txt")
        );
        assert_eq!(
            candidates[1],
            PathBuf::from("/Volumes/Kindle/documents/My Clippings.txt")
        );
    }

    #[test]
    fn defaults_pull_destination_to_local_folder() {
        assert_eq!(
            default_pull_destination(),
            PathBuf::from("local").join("my-clippings.txt")
        );
    }

    #[test]
    fn defaults_export_directory_to_clippings_folder() {
        assert_eq!(default_export_directory(), PathBuf::from("clippings"));
    }

    #[test]
    fn finds_first_existing_clippings_file_from_roots() {
        let temp = tempdir().expect("temp dir should exist");
        let kindle = temp.path().join("Kindle");
        fs::create_dir_all(kindle.join("documents")).expect("kindle dir should exist");
        fs::write(
            kindle.join("documents").join("My Clippings.txt"),
            SAMPLE_INPUT,
        )
        .expect("sample clippings should be written");

        let discovered =
            find_kindle_clippings_path_from_roots(&[kindle]).expect("file should be found");

        assert!(discovered.ends_with("documents/My Clippings.txt"));
    }

    #[test]
    fn copies_clippings_and_creates_destination_parent() {
        let temp = tempdir().expect("temp dir should exist");
        let source = temp.path().join("My Clippings.txt");
        let destination = temp.path().join("local").join("copied.txt");
        fs::write(&source, SAMPLE_INPUT).expect("source clippings should be written");

        let original =
            copy_kindle_clippings(Some(&source), &destination).expect("copy should succeed");

        assert_eq!(original, source);
        assert_eq!(
            fs::read_to_string(destination).expect("destination should be readable"),
            SAMPLE_INPUT
        );
    }

    #[test]
    fn wsl_roots_cover_windows_mount_points() {
        let roots = device_roots_for_platform(HostPlatform::Wsl, None);

        assert!(roots.contains(&PathBuf::from("/mnt/c")));
        assert!(roots.contains(&PathBuf::from("/mnt/z")));
    }

    #[test]
    fn writes_single_markdown_file_to_output_directory() {
        let temp = tempdir().expect("temp dir should exist");
        let entries = parse_kindle_clippings(SAMPLE_INPUT).expect("sample input should parse");

        let written = write_markdown_output(&entries, temp.path(), OutputLayout::SingleFile)
            .expect("single file write should succeed");

        assert_eq!(written.len(), 1);
        assert_eq!(written[0], temp.path().join("clippings.md"));
        let output =
            fs::read_to_string(&written[0]).expect("single markdown file should be readable");
        assert!(output.contains("# The Rust Programming Language by Steve Klabnik, Carol Nichols"));
    }

    #[test]
    fn writes_one_markdown_file_per_book() {
        let temp = tempdir().expect("temp dir should exist");
        let input = r#"Book One (Author A) - Your Highlight on page 1 | Location 1-1 | Added on Monday, January 1, 2024 10:00:00 AM

Alpha

==========
Book Two (Author B) - Your Highlight on page 2 | Location 2-2 | Added on Monday, January 1, 2024 11:00:00 AM

Beta

==========
"#;
        let entries = parse_kindle_clippings(input).expect("input should parse");

        let written = write_markdown_output(&entries, temp.path(), OutputLayout::ByBook)
            .expect("per-book write should succeed");

        assert_eq!(written.len(), 2);
        assert!(written.contains(&temp.path().join("book-one-author-a.md")));
        assert!(written.contains(&temp.path().join("book-two-author-b.md")));

        let first = fs::read_to_string(temp.path().join("book-one-author-a.md"))
            .expect("book markdown should be readable");
        assert!(first.contains("# Book One by Author A"));
        assert!(!first.contains("Book Two"));
    }
}
