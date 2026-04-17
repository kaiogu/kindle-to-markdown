use anyhow::Result;
use regex::Regex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KindleEntry {
    pub title: String,
    pub author: String,
    pub entry_type: String,
    pub location: String,
    pub date: String,
    pub content: String,
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
        let entry_content = lines
            .get(1..)
            .unwrap_or(&[])
            .join("\n")
            .trim()
            .to_string();

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

#[cfg(test)]
mod tests {
    use super::{convert_to_markdown, parse_kindle_clippings};

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
}
