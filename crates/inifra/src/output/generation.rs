use std::path::{Path, PathBuf};

use inifra_core::plan::{Action, DirectoryAction, GenerationPlan};
use similar::TextDiff;

use crate::presentation;

const PLAN_ACTION_WIDTH: usize = "skip (already exists)".len();

pub(crate) fn print_plan(plan: &GenerationPlan, target: &std::path::Path, show_content: bool) {
    let target = display_target(target);
    println!(
        "{}  {}",
        presentation::paint_emphasis("Preview", presentation::Tone::Accent),
        presentation::paint(
            presentation::terminal_text(&target.display().to_string()),
            presentation::Tone::Path,
            false
        )
    );
    println!();
    println!(
        "{}",
        presentation::paint_emphasis("Plan", presentation::Tone::Accent)
    );
    for directory in &plan.directories {
        let (action, tone) = match directory.action {
            DirectoryAction::Create => ("create directory", presentation::Tone::Success),
            DirectoryAction::Existing => ("skip (already exists)", presentation::Tone::Muted),
        };
        print_plan_entry(action, tone, &format!("{}/", directory.path.display()));
    }

    for file in &plan.files {
        let (action, tone) = match file.action {
            Action::Create => ("create", presentation::Tone::Success),
            Action::Replace => ("replace", presentation::Tone::Warning),
            Action::Skip => ("skip (already exists)", presentation::Tone::Muted),
        };
        print_plan_entry(action, tone, &file.path.display().to_string());
    }

    if show_content {
        let content_files = plan
            .files
            .iter()
            .filter(|file| file.action != Action::Skip)
            .collect::<Vec<_>>();
        println!();
        println!(
            "{}",
            presentation::paint_emphasis("Content", presentation::Tone::Accent)
        );
        if content_files.is_empty() {
            let message = if plan.files.is_empty() {
                "No file content to preview."
            } else {
                "All existing files are skipped. Pass --overwrite to preview replacements."
            };
            println!(
                "{}",
                presentation::paint(message, presentation::Tone::Muted, false)
            );
        }

        for file in content_files {
            println!();
            match file.action {
                Action::Create => {
                    println!(
                        "{}",
                        presentation::paint(
                            format!(
                                "--- {} ---",
                                presentation::terminal_text(&file.path.display().to_string())
                            ),
                            presentation::Tone::Path,
                            false
                        )
                    );
                    print_rendered_content(&file.path, &file.contents);
                }
                Action::Replace => {
                    if let Some(existing) = file.existing_contents.as_deref() {
                        print_replacement_preview(&file.path, existing, &file.contents);
                    }
                }
                Action::Skip => {}
            }
        }
    }
}

fn print_plan_entry(action: &str, tone: presentation::Tone, path: &str) {
    println!(
        "  {}  {}",
        presentation::paint(format!("{action:<PLAN_ACTION_WIDTH$}"), tone, false),
        presentation::paint_emphasis(
            presentation::terminal_text(path),
            presentation::Tone::Accent
        )
    );
}

fn print_replacement_preview(path: &Path, existing: &[u8], proposed: &[u8]) {
    let (Ok(existing), Ok(proposed)) =
        (std::str::from_utf8(existing), std::str::from_utf8(proposed))
    else {
        println!(
            "{}",
            presentation::paint_emphasis(
                presentation::terminal_text(&path.display().to_string()),
                presentation::Tone::Accent
            )
        );
        println!(
            "  {}",
            presentation::paint(
                format!(
                    "Binary output: {} → {} bytes",
                    existing.len(),
                    proposed.len()
                ),
                presentation::Tone::Muted,
                false
            )
        );
        return;
    };

    if existing == proposed {
        println!(
            "{}  {}",
            presentation::paint_emphasis(
                presentation::terminal_text(&path.display().to_string()),
                presentation::Tone::Accent
            ),
            presentation::paint("no content changes", presentation::Tone::Muted, false)
        );
        return;
    }

    let path = presentation::terminal_text(&path.display().to_string());
    let diff = TextDiff::from_lines(existing, proposed)
        .unified_diff()
        .context_radius(3)
        .header(&format!("a/{path}"), &format!("b/{path}"))
        .to_string();
    for line in diff.lines() {
        let tone = match line {
            line if line.starts_with("--- ") || line.starts_with("+++ ") => {
                presentation::Tone::Muted
            }
            line if line.starts_with("@@") => presentation::Tone::Accent,
            line if line.starts_with('+') => presentation::Tone::Success,
            line if line.starts_with('-') => presentation::Tone::Error,
            _ => presentation::Tone::Secondary,
        };
        println!(
            "  {}",
            presentation::paint(presentation::terminal_text(line), tone, false)
        );
    }
}

fn display_target(target: &Path) -> PathBuf {
    if let Ok(absolute) = target.canonicalize() {
        return absolute;
    }

    if target.is_absolute() {
        target.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|current| current.join(target))
            .unwrap_or_else(|_| target.to_path_buf())
    }
}

fn print_rendered_content(path: &Path, contents: &[u8]) {
    const INDENT: &str = "  ";

    match std::str::from_utf8(contents) {
        Ok("") => println!("{INDENT}"),
        Ok(content) => {
            let is_markdown = path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| {
                    extension.eq_ignore_ascii_case("md")
                        || extension.eq_ignore_ascii_case("markdown")
                });
            let mut in_code_block = false;
            for line in content.lines() {
                let safe_line = presentation::terminal_text(line);
                let rendered = if is_markdown {
                    color_markdown_line(&safe_line, &mut in_code_block)
                } else {
                    safe_line
                };
                println!("{INDENT}{rendered}");
            }
        }
        Err(_) => println!("{INDENT}[binary output: {} bytes]", contents.len()),
    }
}

fn color_markdown_line(line: &str, in_code_block: &mut bool) -> String {
    let trimmed = line.trim_start();
    let indentation = &line[..line.len() - trimmed.len()];
    if trimmed.starts_with("```") {
        *in_code_block = !*in_code_block;
        return format!(
            "{indentation}{}",
            presentation::paint(trimmed, presentation::Tone::Muted, false)
        );
    }

    if *in_code_block {
        return format!(
            "{indentation}{}",
            presentation::paint(trimmed, presentation::Tone::Secondary, false)
        );
    }

    if trimmed.starts_with('#') {
        return presentation::paint_emphasis(line, presentation::Tone::Warning);
    }

    if trimmed.starts_with("<!--") {
        return presentation::paint(line, presentation::Tone::Muted, false);
    }

    if let Some((marker, rest)) = ["- ", "* ", "+ "]
        .into_iter()
        .find_map(|marker| trimmed.strip_prefix(marker).map(|rest| (marker, rest)))
    {
        return format!(
            "{indentation}{}{}",
            presentation::paint(marker, presentation::Tone::Success, false),
            color_inline_code(rest)
        );
    }

    if let Some(rest) = trimmed.strip_prefix("> ") {
        return format!(
            "{indentation}{}{}",
            presentation::paint("> ", presentation::Tone::Muted, false),
            color_inline_code(rest)
        );
    }
    color_inline_code(line)
}

fn color_inline_code(line: &str) -> String {
    if line.trim().is_empty() {
        return line.to_owned();
    }

    let mut result = String::with_capacity(line.len());
    let mut remaining = line;
    while let Some(start) = remaining.find('`') {
        result.push_str(&presentation::paint(
            &remaining[..start],
            presentation::Tone::Secondary,
            false,
        ));
        let code_start = start + 1;
        let Some(end) = remaining[code_start..].find('`') else {
            result.push_str(&presentation::paint(
                &remaining[start..],
                presentation::Tone::Secondary,
                false,
            ));
            break;
        };

        let code_end = code_start + end;
        result.push_str(&presentation::paint("`", presentation::Tone::Muted, false));
        result.push_str(&presentation::paint(
            &remaining[code_start..code_end],
            presentation::Tone::Template,
            false,
        ));
        result.push_str(&presentation::paint("`", presentation::Tone::Muted, false));
        remaining = &remaining[code_end + 1..];
    }
    result.push_str(&presentation::paint(
        remaining,
        presentation::Tone::Secondary,
        false,
    ));
    result
}

pub(crate) fn print_applied(plan: &GenerationPlan, target: &Path) {
    println!(
        "{} {}",
        presentation::paint_emphasis("Target:", presentation::Tone::Accent),
        presentation::paint(
            presentation::terminal_text(&target.display().to_string()),
            presentation::Tone::Path,
            false
        )
    );
    for directory in &plan.directories {
        if directory.action == DirectoryAction::Create {
            println!(
                "{}  {}/",
                presentation::paint("created directory", presentation::Tone::Success, false),
                presentation::paint(
                    presentation::terminal_text(&directory.path.display().to_string()),
                    presentation::Tone::Path,
                    false
                )
            );
        }
    }

    for file in &plan.files {
        let (action, tone) = match file.action {
            Action::Create => ("created", presentation::Tone::Success),
            Action::Replace => ("replaced", presentation::Tone::Warning),
            Action::Skip => ("already exists", presentation::Tone::Muted),
        };
        println!(
            "{}  {}",
            presentation::paint(action, tone, false),
            presentation::paint(
                presentation::terminal_text(&file.path.display().to_string()),
                presentation::Tone::Path,
                false
            )
        );
    }
}
