use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use tempfile::tempdir;

fn cli_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_kindle-to-markdown"))
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn sample_input_path() -> PathBuf {
    repo_root().join("sample_clippings.txt")
}

fn standard_clippings_input() -> &'static str {
    r#"The Rust Programming Language (Steve Klabnik and Carol Nichols) - Your Highlight on page 23 | Location 234-236 | Added on Friday, August 9, 2024 12:34:56 PM

Memory safety is one of Rust's main selling points.

==========
The Rust Programming Language (Steve Klabnik and Carol Nichols) - Your Note on page 45 | Location 456 | Added on Friday, August 9, 2024 1:15:30 PM

This is really important for systems programming.

==========
"#
}

fn write_standard_input_file(temp_dir: &Path) -> PathBuf {
    let input = temp_dir.join("clippings.txt");
    fs::write(&input, standard_clippings_input()).expect("test input should be written");
    input
}

fn write_settings(config_home: &Path, content: &str) -> PathBuf {
    let settings_path = config_home.join("kindle-to-markdown").join("settings.toml");
    fs::create_dir_all(
        settings_path
            .parent()
            .expect("settings path should have a parent directory"),
    )
    .expect("settings directory should be created");
    fs::write(&settings_path, content).expect("settings file should be written");
    settings_path
}

fn run_cli(args: &[&str]) -> Output {
    Command::new(cli_binary())
        .args(args)
        .output()
        .expect("binary should run")
}

fn run_cli_with_stdin(args: &[&str], stdin: &str) -> Output {
    let mut child = Command::new(cli_binary())
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("binary should run");

    child
        .stdin
        .as_mut()
        .expect("stdin should be piped")
        .write_all(stdin.as_bytes())
        .expect("stdin should be written");

    child.wait_with_output().expect("process should finish")
}

#[test]
fn writes_markdown_to_stdout_and_stats_to_stderr_for_file_input() {
    let temp = tempdir().expect("temp dir should exist");
    let input = write_standard_input_file(temp.path());
    let output = run_cli(&[input.to_str().expect("utf-8 path expected")]);

    assert!(output.status.success(), "process failed: {output:?}");

    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf-8");
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf-8");

    assert!(stdout.contains("# The Rust Programming Language by Steve Klabnik and Carol Nichols"));
    assert!(!stdout.contains("Statistics:"));
    assert!(stderr.contains("Statistics: 2 entries across 1 books"));
    assert!(!stderr.contains("# The Rust Programming Language by Steve Klabnik and Carol Nichols"));
}

#[test]
fn reads_stdin_when_no_input_path_is_given() {
    let stdin = standard_clippings_input();
    let output = run_cli_with_stdin(&[], &stdin);

    assert!(output.status.success(), "process failed: {output:?}");

    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf-8");
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf-8");

    assert!(stdout.contains("> Memory safety is one of Rust's main selling points."));
    assert!(stderr.contains("Statistics: 2 entries across 1 books"));
}

#[test]
fn rejects_conflicting_file_and_discover_inputs() {
    let output = run_cli(&[
        "--discover",
        sample_input_path().to_str().expect("utf-8 path expected"),
    ]);

    assert!(!output.status.success(), "process unexpectedly succeeded");

    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf-8");
    assert!(stderr.contains("cannot use an input file with --discover"));
}

#[test]
fn cli_flags_override_settings_for_output_and_stats() {
    let temp = tempdir().expect("temp dir should exist");
    let config_home = temp.path().join("config-home");
    let input = write_standard_input_file(temp.path());
    let explicit_output = temp.path().join("explicit").join("highlights.md");
    write_settings(
        &config_home,
        r#"
layout = "by-book"
output = "from-settings"
no-stats = false
"#,
    );

    let output = Command::new(cli_binary())
        .current_dir(temp.path())
        .env("XDG_CONFIG_HOME", &config_home)
        .arg(&input)
        .arg("--layout")
        .arg("single")
        .arg("--output")
        .arg(&explicit_output)
        .arg("--no-stats")
        .output()
        .expect("binary should run");

    assert!(output.status.success(), "process failed: {output:?}");
    assert!(
        explicit_output.is_file(),
        "explicit output should be written"
    );
    assert!(
        !temp.path().join("from-settings").exists(),
        "settings output should be overridden by CLI output"
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf-8");
    assert!(stdout.contains("Exported 2 entries into 1 file(s) under"));

    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf-8");
    assert!(
        stderr.is_empty(),
        "stderr should be empty when --no-stats is set"
    );
}

#[test]
fn writes_one_markdown_file_per_book_to_output_directory() {
    let temp = tempdir().expect("temp dir should exist");
    let input = temp.path().join("multi-book.txt");
    let output_dir = temp.path().join("exports");

    fs::write(
        &input,
        r#"Book One (Author A) - Your Highlight on page 1 | Location 1-1 | Added on Monday, January 1, 2024 10:00:00 AM

Alpha

==========
Book Two (Author B) - Your Note on page 2 | Location 2-2 | Added on Monday, January 1, 2024 11:00:00 AM

Beta

==========
"#,
    )
    .expect("test input should be written");

    let output = Command::new(cli_binary())
        .arg(&input)
        .arg("--layout")
        .arg("by-book")
        .arg("--output")
        .arg(&output_dir)
        .output()
        .expect("binary should run");

    assert!(output.status.success(), "process failed: {output:?}");
    assert!(output_dir.join("book-one-author-a.md").is_file());
    assert!(output_dir.join("book-two-author-b.md").is_file());

    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf-8");
    assert!(stdout.contains("Exported 2 entries into 2 file(s) under"));
}
