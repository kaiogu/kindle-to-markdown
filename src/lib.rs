pub mod settings;

use anyhow::{Context, Result, anyhow, bail};
use regex::Regex;
use serde::Serialize;
use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Book,
    Date,
    Location,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatsFormat {
    Text,
    Totals,
    Json,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputTarget {
    File(PathBuf),
    Directory(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BookStats {
    pub title: String,
    pub author: String,
    pub highlights: usize,
    pub notes: usize,
    pub bookmarks: usize,
    pub other: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StatsTotals {
    pub entries: usize,
    pub books: usize,
    pub highlights: usize,
    pub notes: usize,
    pub bookmarks: usize,
    pub other: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StatsReport {
    pub totals: StatsTotals,
    pub books: Vec<BookStats>,
    pub top_books: Vec<BookStats>,
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

pub fn resolve_output_target(layout: OutputLayout, output: Option<&Path>) -> OutputTarget {
    match (layout, output) {
        (OutputLayout::SingleFile, Some(path)) => {
            OutputTarget::File(path.to_path_buf())
        }
        (_, Some(path)) => OutputTarget::Directory(path.to_path_buf()),
        _ => OutputTarget::Directory(default_export_directory()),
    }
}

pub fn raw_destination_for_output(input_path: &Path, output_target: &OutputTarget) -> PathBuf {
    let raw_name = input_path
        .file_name()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("My Clippings.txt"));

    match output_target {
        OutputTarget::File(path) => path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(raw_name),
        OutputTarget::Directory(path) => path.join(raw_name),
    }
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
            "could not find My Clippings.txt on a connected Kindle. Pass the file path explicitly, or ensure the device mount is visible to this environment before retrying --discover."
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
    // WSL usually exposes Windows removable drives through /mnt/<letter>.
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

    let single_line_re = Regex::new(
        r"^(.+) \((.+)\) - Your (Highlight|Note|Bookmark) on (?:page \d+ \| )?Location ([^|]+) \| Added on (.+)$",
    )?;
    let metadata_re = Regex::new(
        r"^- Your (Highlight|Note|Bookmark) on (?:page \d+ \| )?Location ([^|]+) \| Added on (.+)$",
    )?;

    for chunk in chunks {
        let chunk = chunk.trim().trim_start_matches('\u{feff}');
        if chunk.is_empty() {
            continue;
        }

        let lines: Vec<&str> = chunk.lines().collect();
        if lines.is_empty() {
            continue;
        }

        let title_line = lines[0].trim().trim_start_matches('\u{feff}');

        if let Some(captures) = single_line_re.captures(title_line) {
            let entry_content = lines.get(1..).unwrap_or(&[]).join("\n").trim().to_string();
            entries.push(KindleEntry {
                title: captures[1].to_string(),
                author: captures[2].to_string(),
                entry_type: captures[3].to_string(),
                location: captures[4].trim().to_string(),
                date: captures[5].trim().to_string(),
                content: entry_content,
            });
            continue;
        }

        let metadata_line = lines.get(1).map(|line| line.trim());
        let entry_content = lines.get(2..).unwrap_or(&[]).join("\n").trim().to_string();

        if let (Some((title, author)), Some(metadata)) =
            (parse_title_and_author(title_line), metadata_line)
            && let Some(details) = metadata_re.captures(metadata)
        {
            let entry = KindleEntry {
                title,
                author,
                entry_type: details[1].to_string(),
                location: details[2].trim().to_string(),
                date: details[3].trim().to_string(),
                content: entry_content,
            };
            entries.push(entry);
        }
    }

    Ok(entries)
}

pub fn process_entries(
    entries: Vec<KindleEntry>,
    sort_key: Option<SortKey>,
    dedupe: bool,
) -> Vec<KindleEntry> {
    let mut processed = if dedupe {
        dedupe_entries(entries)
    } else {
        entries
    };

    if let Some(sort_key) = sort_key {
        sort_entries(&mut processed, sort_key);
    }

    processed
}

fn dedupe_entries(entries: Vec<KindleEntry>) -> Vec<KindleEntry> {
    let mut seen = BTreeSet::new();
    let mut deduped = Vec::with_capacity(entries.len());

    for entry in entries {
        let key = (
            entry.title.clone(),
            entry.author.clone(),
            entry.entry_type.clone(),
            entry.location.clone(),
            entry.date.clone(),
            entry.content.clone(),
        );

        if seen.insert(key) {
            deduped.push(entry);
        }
    }

    deduped
}

fn sort_entries(entries: &mut [KindleEntry], sort_key: SortKey) {
    match sort_key {
        SortKey::Book => {
            entries.sort_by(compare_book);
        }
        SortKey::Date => {
            let book_order = encounter_book_order(entries);
            entries.sort_by(|left, right| {
                compare_within_book(left, right, &book_order, |left, right| {
                    left.date.cmp(&right.date)
                })
            });
        }
        SortKey::Location => {
            let book_order = encounter_book_order(entries);
            entries.sort_by(|left, right| {
                compare_within_book(left, right, &book_order, compare_location)
            });
        }
    }
}

fn encounter_book_order(entries: &[KindleEntry]) -> HashMap<(String, String), usize> {
    let mut order = HashMap::new();

    for entry in entries {
        let next_index = order.len();
        order
            .entry((entry.title.clone(), entry.author.clone()))
            .or_insert(next_index);
    }

    order
}

fn compare_book(left: &KindleEntry, right: &KindleEntry) -> Ordering {
    normalize_text(&left.title)
        .cmp(&normalize_text(&right.title))
        .then_with(|| normalize_text(&left.author).cmp(&normalize_text(&right.author)))
}

fn compare_within_book(
    left: &KindleEntry,
    right: &KindleEntry,
    book_order: &HashMap<(String, String), usize>,
    compare_entry: impl Fn(&KindleEntry, &KindleEntry) -> Ordering,
) -> Ordering {
    let left_book = (left.title.clone(), left.author.clone());
    let right_book = (right.title.clone(), right.author.clone());
    let left_order = book_order.get(&left_book).copied().unwrap_or(usize::MAX);
    let right_order = book_order.get(&right_book).copied().unwrap_or(usize::MAX);

    left_order
        .cmp(&right_order)
        .then_with(|| compare_entry(left, right))
}

fn compare_location(left: &KindleEntry, right: &KindleEntry) -> Ordering {
    location_sort_key(&left.location)
        .cmp(&location_sort_key(&right.location))
        .then_with(|| left.location.cmp(&right.location))
}

fn location_sort_key(location: &str) -> (Option<u32>, String) {
    (
        parse_location_start(location),
        normalize_text(location).into_owned(),
    )
}

fn parse_location_start(location: &str) -> Option<u32> {
    let digits: String = location
        .chars()
        .skip_while(|ch| !ch.is_ascii_digit())
        .take_while(|ch| ch.is_ascii_digit())
        .collect();

    if digits.is_empty() {
        None
    } else {
        digits.parse().ok()
    }
}

fn normalize_text(value: &str) -> std::borrow::Cow<'_, str> {
    if value.chars().any(|ch| ch.is_ascii_uppercase()) {
        std::borrow::Cow::Owned(value.to_ascii_lowercase())
    } else {
        std::borrow::Cow::Borrowed(value)
    }
}

fn parse_title_and_author(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim();
    let author = trimmed.strip_suffix(')')?;
    let (title, author) = author.rsplit_once(" (")?;

    Some((title.to_string(), author.to_string()))
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

pub fn collect_book_stats(entries: &[KindleEntry]) -> Vec<BookStats> {
    let mut stats = Vec::<BookStats>::new();

    for entry in entries {
        let stat = if let Some(stat) = stats
            .iter_mut()
            .find(|stat| stat.title == entry.title && stat.author == entry.author)
        {
            stat
        } else {
            stats.push(BookStats {
                title: entry.title.clone(),
                author: entry.author.clone(),
                highlights: 0,
                notes: 0,
                bookmarks: 0,
                other: 0,
            });
            stats.last_mut().expect("stats entry was just pushed")
        };

        match entry.entry_type.as_str() {
            "Highlight" => stat.highlights += 1,
            "Note" => stat.notes += 1,
            "Bookmark" => stat.bookmarks += 1,
            _ => stat.other += 1,
        }
    }

    stats
}

pub fn render_book_stats(entries: &[KindleEntry]) -> String {
    render_book_stats_with_format(entries, StatsFormat::Text)
}

pub fn render_book_stats_with_format(entries: &[KindleEntry], format: StatsFormat) -> String {
    let report = build_stats_report(entries);

    match format {
        StatsFormat::Text => render_text_stats(&report),
        StatsFormat::Totals => render_totals_stats(&report.totals),
        StatsFormat::Json => {
            serde_json::to_string_pretty(&report).expect("stats report should serialize")
        }
    }
}

pub fn build_stats_report(entries: &[KindleEntry]) -> StatsReport {
    let books = collect_book_stats(entries);
    let totals = StatsTotals {
        entries: entries.len(),
        books: books.len(),
        highlights: books.iter().map(|book| book.highlights).sum(),
        notes: books.iter().map(|book| book.notes).sum(),
        bookmarks: books.iter().map(|book| book.bookmarks).sum(),
        other: books.iter().map(|book| book.other).sum(),
    };

    StatsReport {
        totals,
        top_books: top_books_by_entries(&books, 3),
        books,
    }
}

fn render_totals_stats(totals: &StatsTotals) -> String {
    format!(
        "Statistics: {} entries across {} books (highlights: {}, notes: {}, bookmarks: {}, other: {})",
        totals.entries,
        totals.books,
        totals.highlights,
        totals.notes,
        totals.bookmarks,
        totals.other
    )
}

fn render_text_stats(report: &StatsReport) -> String {
    let mut output = format!("{}\n", render_totals_stats(&report.totals));

    for book in &report.books {
        output.push_str(&format!(
            "- {} by {}: highlights {}, notes {}, bookmarks {}, other {}\n",
            book.title, book.author, book.highlights, book.notes, book.bookmarks, book.other
        ));
    }

    if !report.top_books.is_empty() {
        output.push_str("Top books by entries:\n");
        for book in &report.top_books {
            output.push_str(&format!(
                "- {} by {}: {} entries\n",
                book.title,
                book.author,
                total_book_entries(book)
            ));
        }
    }

    output.trim_end().to_string()
}

fn top_books_by_entries(book_stats: &[BookStats], limit: usize) -> Vec<BookStats> {
    let mut books = book_stats.to_vec();
    books.sort_by(|left, right| {
        total_book_entries(right)
            .cmp(&total_book_entries(left))
            .then_with(|| normalize_text(&left.title).cmp(&normalize_text(&right.title)))
            .then_with(|| normalize_text(&left.author).cmp(&normalize_text(&right.author)))
    });
    books.truncate(limit);
    books
}

fn total_book_entries(book: &BookStats) -> usize {
    book.highlights + book.notes + book.bookmarks + book.other
}

pub fn write_markdown_output(
    entries: &[KindleEntry],
    output_target: &OutputTarget,
    layout: OutputLayout,
) -> Result<Vec<PathBuf>> {
    match layout {
        OutputLayout::SingleFile => write_single_markdown_file(entries, output_target),
        OutputLayout::ByBook => write_markdown_files_by_book(entries, output_target),
    }
}

fn write_single_markdown_file(
    entries: &[KindleEntry],
    output_target: &OutputTarget,
) -> Result<Vec<PathBuf>> {
    let destination = match output_target {
        OutputTarget::File(path) => path.clone(),
        OutputTarget::Directory(path) => path.join("clippings.md"),
    };

    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create output directory {}", parent.display()))?;
    }

    fs::write(&destination, convert_to_markdown(entries))
        .with_context(|| format!("failed to write markdown file {}", destination.display()))?;

    Ok(vec![destination])
}

fn write_markdown_files_by_book(
    entries: &[KindleEntry],
    output_target: &OutputTarget,
) -> Result<Vec<PathBuf>> {
    let output_dir = match output_target {
        OutputTarget::Directory(path) => path,
        OutputTarget::File(path) => {
            bail!(
                "by-book layout requires a directory output, got file {}",
                path.display()
            );
        }
    };

    fs::create_dir_all(output_dir)
        .with_context(|| format!("failed to create output directory {}", output_dir.display()))?;

    let mut grouped_entries: Vec<((String, String), Vec<KindleEntry>)> = Vec::new();
    for entry in entries {
        let book = (entry.title.clone(), entry.author.clone());
        if let Some((_, book_entries)) = grouped_entries.iter_mut().find(|(key, _)| *key == book) {
            book_entries.push(entry.clone());
        } else {
            grouped_entries.push((book, vec![entry.clone()]));
        }
    }

    let mut written = Vec::new();
    let mut used_slugs = HashMap::new();
    for ((title, author), book_entries) in grouped_entries {
        written.push(write_book_markdown_file(
            output_dir,
            &title,
            &author,
            &book_entries,
            &mut used_slugs,
        )?);
    }

    Ok(written)
}

fn write_book_markdown_file(
    output_dir: &Path,
    title: &str,
    author: &str,
    entries: &[KindleEntry],
    used_slugs: &mut HashMap<String, usize>,
) -> Result<PathBuf> {
    let file_name = format!("{}.md", unique_book_slug(title, author, used_slugs));
    let destination = output_dir.join(file_name);
    fs::write(&destination, convert_to_markdown(entries))
        .with_context(|| format!("failed to write markdown file {}", destination.display()))?;

    Ok(destination)
}

fn slugify_book_title(title: &str, author: &str) -> String {
    const MAX_TITLE_SLUG_CHARS: usize = 56;
    const MAX_AUTHOR_SLUG_CHARS: usize = 20;
    const MAX_BOOK_SLUG_CHARS: usize = 80;

    let title_slug = slugify_segment(title, MAX_TITLE_SLUG_CHARS);
    let author_slug = slugify_segment(author, MAX_AUTHOR_SLUG_CHARS);

    let mut slug = if title_slug.is_empty() {
        "book".to_string()
    } else {
        title_slug
    };

    if !author_slug.is_empty() {
        slug.push_str("-by-");
        slug.push_str(&author_slug);
    }

    trim_slug_to_length(&slug, MAX_BOOK_SLUG_CHARS)
}

fn slugify_segment(value: &str, max_chars: usize) -> String {
    let lowercase = value.to_lowercase();
    let mut slug = String::with_capacity(lowercase.len());
    let mut last_was_separator = false;

    for ch in lowercase.chars() {
        let is_word = ch.is_alphanumeric();
        if is_word {
            slug.push(ch);
            last_was_separator = false;
        } else if !last_was_separator {
            slug.push('-');
            last_was_separator = true;
        }
    }

    trim_slug_to_length(&slug, max_chars)
}

fn trim_slug_to_length(slug: &str, max_chars: usize) -> String {
    let trimmed = slug.trim_matches('-');
    if trimmed.is_empty() {
        return "book".to_string();
    }

    let mut shortened = String::new();
    for ch in trimmed.chars().take(max_chars) {
        shortened.push(ch);
    }

    let shortened = shortened.trim_matches('-');
    if shortened.is_empty() {
        "book".to_string()
    } else {
        shortened.to_string()
    }
}

fn unique_book_slug(title: &str, author: &str, used_slugs: &mut HashMap<String, usize>) -> String {
    const MAX_BOOK_SLUG_CHARS: usize = 80;

    let base = slugify_book_title(title, author);
    let next = used_slugs.entry(base.clone()).or_insert(0);
    *next += 1;

    if *next == 1 {
        base
    } else {
        let suffix = format!("-{}", next);
        let available = MAX_BOOK_SLUG_CHARS.saturating_sub(suffix.chars().count());
        let prefix = trim_slug_to_length(&base, available);
        format!("{prefix}{suffix}")
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BookStats, HostPlatform, KindleEntry, OutputLayout, OutputTarget, SortKey, StatsFormat,
        clippings_paths_for_device_root, collect_book_stats, convert_to_markdown,
        copy_kindle_clippings, default_export_directory, default_pull_destination,
        device_roots_for_platform, find_kindle_clippings_path_from_roots, parse_kindle_clippings,
        parse_title_and_author, process_entries, raw_destination_for_output, render_book_stats,
        render_book_stats_with_format, resolve_output_target, slugify_book_title,
        write_markdown_output,
    };
    use std::fs;
    use std::path::{Path, PathBuf};
    use tempfile::tempdir;

    fn read_fixture(name: &str) -> String {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join(name);
        fs::read_to_string(path).expect("fixture should be readable")
    }

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

    const TWO_LINE_KINDLE_INPUT: &str = "\u{feff}The Sirens of Titan (Kurt Vonnegut)\n- Your Highlight on page 219 | Location 3355-3356 | Added on Friday, November 17, 2023 1:14:42 PM\n\nHe used it in order to assert the friendship he felt for Rumfoord.\n==========\nThe Conscious Mind (Philosophy of Mind) (David J. Chalmers)\n- Your Note on page 64 | Location 981 | Added on Sunday, December 17, 2023 10:10:11 AM\n\nIs it really though?\n==========\n";

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
    fn parses_two_line_kindle_headers_with_bom() {
        let entries =
            parse_kindle_clippings(TWO_LINE_KINDLE_INPUT).expect("two-line input should parse");

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].title, "The Sirens of Titan");
        assert_eq!(entries[0].author, "Kurt Vonnegut");
        assert_eq!(entries[0].entry_type, "Highlight");
        assert_eq!(entries[0].location, "3355-3356");
        assert_eq!(entries[1].title, "The Conscious Mind (Philosophy of Mind)");
        assert_eq!(entries[1].author, "David J. Chalmers");
        assert_eq!(entries[1].entry_type, "Note");
        assert_eq!(entries[1].content, "Is it really though?");
    }

    #[test]
    fn parses_one_line_fixture_with_page_metadata() {
        let entries = parse_kindle_clippings(&read_fixture("one-line-page-present.txt"))
            .expect("fixture should parse");

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].title, "Deep Work");
        assert_eq!(entries[0].author, "Cal Newport");
        assert_eq!(entries[0].entry_type, "Highlight");
        assert_eq!(entries[0].location, "1324-1325");
        assert_eq!(
            entries[0].content,
            "Focus without distraction is rare and valuable."
        );
    }

    #[test]
    fn parses_one_line_fixture_without_page_metadata() {
        let entries = parse_kindle_clippings(&read_fixture("one-line-page-absent.txt"))
            .expect("fixture should parse");

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].entry_type, "Bookmark");
        assert_eq!(entries[0].location, "1500");
        assert_eq!(entries[0].content, "");
        assert_eq!(entries[1].entry_type, "Highlight");
        assert_eq!(entries[1].location, "1234-1237");
        assert_eq!(entries[1].content, "Functions should do one thing.");
    }

    #[test]
    fn parses_two_line_fixture_without_bom() {
        let entries = parse_kindle_clippings(&read_fixture("two-line-no-bom.txt"))
            .expect("fixture should parse");

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].title, "The Pragmatic Programmer");
        assert_eq!(entries[0].author, "Andy Hunt, Dave Thomas");
        assert_eq!(entries[0].entry_type, "Note");
        assert_eq!(entries[0].location, "610-611");
        assert_eq!(entries[0].content, "Good reminder about feedback loops.");
    }

    #[test]
    fn parses_two_line_fixture_with_bom() {
        let entries = parse_kindle_clippings(&read_fixture("two-line-bom.txt"))
            .expect("fixture should parse");

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].title, "The Sirens of Titan");
        assert_eq!(entries[0].author, "Kurt Vonnegut");
        assert_eq!(entries[0].entry_type, "Highlight");
        assert_eq!(entries[0].location, "3355-3356");
        assert_eq!(
            entries[0].content,
            "He used it in order to assert the friendship he felt for Rumfoord."
        );
    }

    #[test]
    fn parses_bookmark_only_fixture() {
        let entries = parse_kindle_clippings(&read_fixture("bookmark-only.txt"))
            .expect("fixture should parse");

        assert_eq!(entries.len(), 2);
        assert!(entries.iter().all(|entry| entry.entry_type == "Bookmark"));
        assert!(entries.iter().all(|entry| entry.content.is_empty()));
    }

    #[test]
    fn skips_malformed_chunks_in_fixture() {
        let entries = parse_kindle_clippings(&read_fixture("malformed-chunks.txt"))
            .expect("fixture should parse");

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].title, "Refactoring");
        assert_eq!(entries[1].title, "Domain-Driven Design");
    }

    #[test]
    fn parses_mixed_locale_date_variants_fixture() {
        let entries = parse_kindle_clippings(&read_fixture("mixed-locale-date-variants.txt"))
            .expect("fixture should parse");

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].date, "Sonntag, 17. Dezember 2023 22:10:11");
        assert_eq!(entries[1].date, "sexta-feira, 12 de julho de 2024 14:03:05");
        assert_eq!(entries[1].title, "Cem Anos de Solidao");
        assert_eq!(entries[1].author, "Gabriel Garcia Marquez");
    }

    #[test]
    fn dedupes_repeated_entries_while_keeping_first_occurrence() {
        let entries = vec![
            KindleEntry {
                title: "Deep Work".to_string(),
                author: "Cal Newport".to_string(),
                entry_type: "Highlight".to_string(),
                location: "10-11".to_string(),
                date: "Monday".to_string(),
                content: "Focus".to_string(),
            },
            KindleEntry {
                title: "Deep Work".to_string(),
                author: "Cal Newport".to_string(),
                entry_type: "Highlight".to_string(),
                location: "10-11".to_string(),
                date: "Monday".to_string(),
                content: "Focus".to_string(),
            },
            KindleEntry {
                title: "Deep Work".to_string(),
                author: "Cal Newport".to_string(),
                entry_type: "Note".to_string(),
                location: "12".to_string(),
                date: "Tuesday".to_string(),
                content: "Keep it".to_string(),
            },
        ];

        let processed = process_entries(entries, None, true);

        assert_eq!(processed.len(), 2);
        assert_eq!(processed[0].entry_type, "Highlight");
        assert_eq!(processed[1].entry_type, "Note");
    }

    #[test]
    fn sorts_books_alphabetically() {
        let entries = vec![
            KindleEntry {
                title: "Zoo".to_string(),
                author: "Author Z".to_string(),
                entry_type: "Highlight".to_string(),
                location: "10".to_string(),
                date: "Wednesday".to_string(),
                content: "Last".to_string(),
            },
            KindleEntry {
                title: "Alpha".to_string(),
                author: "Author A".to_string(),
                entry_type: "Highlight".to_string(),
                location: "2".to_string(),
                date: "Monday".to_string(),
                content: "First".to_string(),
            },
        ];

        let processed = process_entries(entries, Some(SortKey::Book), false);

        assert_eq!(processed[0].title, "Alpha");
        assert_eq!(processed[1].title, "Zoo");
    }

    #[test]
    fn sorts_by_location_within_each_book() {
        let entries = vec![
            KindleEntry {
                title: "Book One".to_string(),
                author: "Author".to_string(),
                entry_type: "Highlight".to_string(),
                location: "20-21".to_string(),
                date: "Wednesday".to_string(),
                content: "Later".to_string(),
            },
            KindleEntry {
                title: "Book One".to_string(),
                author: "Author".to_string(),
                entry_type: "Highlight".to_string(),
                location: "3-4".to_string(),
                date: "Monday".to_string(),
                content: "Sooner".to_string(),
            },
            KindleEntry {
                title: "Book Two".to_string(),
                author: "Author".to_string(),
                entry_type: "Highlight".to_string(),
                location: "9".to_string(),
                date: "Tuesday".to_string(),
                content: "Other book".to_string(),
            },
        ];

        let processed = process_entries(entries, Some(SortKey::Location), false);

        assert_eq!(processed[0].title, "Book One");
        assert_eq!(processed[0].location, "3-4");
        assert_eq!(processed[1].location, "20-21");
        assert_eq!(processed[2].title, "Book Two");
    }

    #[test]
    fn sorts_by_date_within_each_book() {
        let entries = vec![
            KindleEntry {
                title: "Book One".to_string(),
                author: "Author".to_string(),
                entry_type: "Highlight".to_string(),
                location: "20-21".to_string(),
                date: "Wednesday".to_string(),
                content: "Later".to_string(),
            },
            KindleEntry {
                title: "Book One".to_string(),
                author: "Author".to_string(),
                entry_type: "Note".to_string(),
                location: "3-4".to_string(),
                date: "Monday".to_string(),
                content: "Sooner".to_string(),
            },
            KindleEntry {
                title: "Book Two".to_string(),
                author: "Author".to_string(),
                entry_type: "Highlight".to_string(),
                location: "9".to_string(),
                date: "Tuesday".to_string(),
                content: "Other book".to_string(),
            },
        ];

        let processed = process_entries(entries, Some(SortKey::Date), false);

        assert_eq!(processed[0].date, "Monday");
        assert_eq!(processed[1].date, "Wednesday");
        assert_eq!(processed[2].title, "Book Two");
    }

    #[test]
    fn parses_title_and_author_using_last_parenthetical_group() {
        let parsed =
            parse_title_and_author("The Conscious Mind (Philosophy of Mind) (David J. Chalmers)")
                .expect("title and author should parse");

        assert_eq!(parsed.0, "The Conscious Mind (Philosophy of Mind)");
        assert_eq!(parsed.1, "David J. Chalmers");
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
    fn resolves_single_layout_file_output_when_markdown_path_is_given() {
        let output = resolve_output_target(
            OutputLayout::SingleFile,
            Some(Path::new("notes/highlights.md")),
        );

        assert_eq!(
            output,
            OutputTarget::File(PathBuf::from("notes/highlights.md"))
        );
    }

    #[test]
    fn resolves_single_layout_file_output_even_without_markdown_extension() {
        let output =
            resolve_output_target(OutputLayout::SingleFile, Some(Path::new("notes.txt")));

        assert_eq!(output, OutputTarget::File(PathBuf::from("notes.txt")));
    }

    #[test]
    fn resolves_by_book_output_as_directory() {
        let output =
            resolve_output_target(OutputLayout::ByBook, Some(Path::new("notes/highlights.md")));

        assert_eq!(
            output,
            OutputTarget::Directory(PathBuf::from("notes/highlights.md"))
        );
    }

    #[test]
    fn derives_raw_destination_next_to_single_markdown_file() {
        let destination = raw_destination_for_output(
            Path::new("/tmp/My Clippings.txt"),
            &OutputTarget::File(PathBuf::from("notes/highlights.md")),
        );

        assert_eq!(destination, PathBuf::from("notes/My Clippings.txt"));
    }

    #[test]
    fn derives_raw_destination_inside_output_directory() {
        let destination = raw_destination_for_output(
            Path::new("/tmp/My Clippings.txt"),
            &OutputTarget::Directory(PathBuf::from("clippings")),
        );

        assert_eq!(destination, PathBuf::from("clippings/My Clippings.txt"));
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

        let written = write_markdown_output(
            &entries,
            &OutputTarget::Directory(temp.path().to_path_buf()),
            OutputLayout::SingleFile,
        )
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

        let written = write_markdown_output(
            &entries,
            &OutputTarget::Directory(temp.path().to_path_buf()),
            OutputLayout::ByBook,
        )
        .expect("per-book write should succeed");

        assert_eq!(written.len(), 2);
        assert!(written.contains(&temp.path().join("book-one-by-author-a.md")));
        assert!(written.contains(&temp.path().join("book-two-by-author-b.md")));

        let first = fs::read_to_string(temp.path().join("book-one-by-author-a.md"))
            .expect("book markdown should be readable");
        assert!(first.contains("# Book One by Author A"));
        assert!(!first.contains("Book Two"));
    }

    #[test]
    fn shortens_book_slugs_and_keeps_author_context() {
        let slug = slugify_book_title(
            "A Very Long Book Title That Keeps Going Past Reasonable Filename Length Limits",
            "An Author With A Surprisingly Long Name",
        );

        assert!(slug.len() <= 80);
        assert!(slug.starts_with("a-very-long-book-title"));
        assert!(slug.contains("-by-an-author-with-a"));
    }

    #[test]
    fn adds_numeric_suffix_when_book_slugs_collide() {
        let temp = tempdir().expect("temp dir should exist");
        let input = r#"Rust / Book (Author A) - Your Highlight on Location 1 | Added on Monday

Alpha

==========
Rust: Book (Author A) - Your Highlight on Location 2 | Added on Tuesday

Beta

==========
"#;
        let entries = parse_kindle_clippings(input).expect("input should parse");

        let written = write_markdown_output(
            &entries,
            &OutputTarget::Directory(temp.path().to_path_buf()),
            OutputLayout::ByBook,
        )
        .expect("per-book write should succeed");

        assert_eq!(written.len(), 2);
        assert!(written.contains(&temp.path().join("rust-book-by-author-a.md")));
        assert!(written.contains(&temp.path().join("rust-book-by-author-a-2.md")));
    }

    #[test]
    fn groups_non_contiguous_entries_for_the_same_book_into_one_file() {
        let temp = tempdir().expect("temp dir should exist");
        let input = r#"Book One (Author A) - Your Highlight on page 1 | Location 1-1 | Added on Monday, January 1, 2024 10:00:00 AM

Alpha

==========
Book Two (Author B) - Your Highlight on page 2 | Location 2-2 | Added on Monday, January 1, 2024 11:00:00 AM

Beta

==========
Book One (Author A) - Your Note on page 3 | Location 3-3 | Added on Monday, January 1, 2024 12:00:00 PM

Gamma

==========
"#;
        let entries = parse_kindle_clippings(input).expect("input should parse");

        let written = write_markdown_output(
            &entries,
            &OutputTarget::Directory(temp.path().to_path_buf()),
            OutputLayout::ByBook,
        )
        .expect("per-book write should succeed");

        assert_eq!(written.len(), 2);
        let first = fs::read_to_string(temp.path().join("book-one-by-author-a.md"))
            .expect("book markdown should be readable");
        assert!(first.contains("> Alpha"));
        assert!(first.contains("**Note:** Gamma"));
        assert!(!first.contains("Beta"));
    }

    #[test]
    fn writes_single_markdown_file_to_explicit_file_path() {
        let temp = tempdir().expect("temp dir should exist");
        let entries = parse_kindle_clippings(SAMPLE_INPUT).expect("sample input should parse");
        let output = temp.path().join("exports").join("highlights.md");

        let written = write_markdown_output(
            &entries,
            &OutputTarget::File(output.clone()),
            OutputLayout::SingleFile,
        )
        .expect("single-file output path should succeed");

        assert_eq!(written, vec![output.clone()]);
        let rendered = fs::read_to_string(output).expect("explicit markdown file should exist");
        assert!(
            rendered.contains("# The Rust Programming Language by Steve Klabnik, Carol Nichols")
        );
    }

    #[test]
    fn collects_stats_per_book_and_entry_type() {
        let entries = parse_kindle_clippings(TWO_LINE_KINDLE_INPUT).expect("input should parse");

        let stats = collect_book_stats(&entries);

        assert_eq!(
            stats,
            vec![
                BookStats {
                    title: "The Sirens of Titan".to_string(),
                    author: "Kurt Vonnegut".to_string(),
                    highlights: 1,
                    notes: 0,
                    bookmarks: 0,
                    other: 0,
                },
                BookStats {
                    title: "The Conscious Mind (Philosophy of Mind)".to_string(),
                    author: "David J. Chalmers".to_string(),
                    highlights: 0,
                    notes: 1,
                    bookmarks: 0,
                    other: 0,
                },
            ]
        );
    }

    #[test]
    fn renders_stats_summary_and_book_lines() {
        let entries = parse_kindle_clippings(TWO_LINE_KINDLE_INPUT).expect("input should parse");
        let rendered = render_book_stats(&entries);

        assert!(rendered.contains("Statistics: 2 entries across 2 books"));
        assert!(rendered.contains(
            "- The Sirens of Titan by Kurt Vonnegut: highlights 1, notes 0, bookmarks 0, other 0"
        ));
        assert!(rendered.contains("- The Conscious Mind (Philosophy of Mind) by David J. Chalmers: highlights 0, notes 1, bookmarks 0, other 0"));
        assert!(rendered.contains("Top books by entries:"));
    }

    #[test]
    fn renders_totals_only_stats() {
        let entries = parse_kindle_clippings(TWO_LINE_KINDLE_INPUT).expect("input should parse");
        let rendered = render_book_stats_with_format(&entries, StatsFormat::Totals);

        assert!(rendered.contains("Statistics: 2 entries across 2 books"));
        assert!(!rendered.contains("Top books by entries:"));
        assert!(!rendered.contains("The Sirens of Titan by Kurt Vonnegut"));
    }

    #[test]
    fn renders_json_stats() {
        let entries = parse_kindle_clippings(TWO_LINE_KINDLE_INPUT).expect("input should parse");
        let rendered = render_book_stats_with_format(&entries, StatsFormat::Json);

        assert!(rendered.contains("\"totals\""));
        assert!(rendered.contains("\"top_books\""));
        assert!(rendered.contains("\"title\": \"The Sirens of Titan\""));
    }
}
