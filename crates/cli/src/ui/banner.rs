//! No-argument welcome screen and its shared layout.

use crate::ui::{ACCENT, BOLD, MUTED, outln};
use std::io::IsTerminal;

/// ANSI Shadow wordmark; 82 columns at its widest.
const WIDE_LOGO: &str = "\
██████╗ ██╗      ██████╗  ██████╗██╗  ██╗███████╗ ██████╗ ██████╗ ██████╗ ███████╗
██╔══██╗██║     ██╔═══██╗██╔════╝██║ ██╔╝██╔════╝██╔════╝██╔═══██╗██╔══██╗██╔════╝
██████╔╝██║     ██║   ██║██║     █████╔╝ ███████╗██║     ██║   ██║██████╔╝█████╗
██╔══██╗██║     ██║   ██║██║     ██╔═██╗ ╚════██║██║     ██║   ██║██╔═══╝ ██╔══╝
██████╔╝███████╗╚██████╔╝╚██████╗██║  ██╗███████║╚██████╗╚██████╔╝██║     ███████╗
╚═════╝ ╚══════╝ ╚═════╝  ╚═════╝╚═╝  ╚═╝╚══════╝ ╚═════╝ ╚═════╝ ╚═╝     ╚══════╝";

/// Standard figlet wordmark; 59 columns at its widest.
const COMPACT_LOGO: &str = r#"
 ____  _     ___   ____ _  ______   ____ ___  ____  _____
| __ )| |   / _ \ / ___| |/ / ___| / ___/ _ \|  _ \| ____|
|  _ \| |  | | | | |   | ' /\___ \| |  | | | | |_) |  _|
| |_) | |__| |_| | |___| . \ ___) | |__| |_| |  __/| |___
|____/|_____\___/ \____|_|\_\____/ \____\___/|_|   |_____|"#;

/// Small figlet wordmark; 47 columns at its widest.
const SMALL_LOGO: &str = r#"
 ___ _    ___   ___ _  _____  ___ ___  ___ ___
| _ ) |  / _ \ / __| |/ / __|/ __/ _ \| _ \ __|
| _ \ |_| (_) | (__| ' <\__ \ (_| (_) |  _/ _|
|___/____\___/ \___|_|\_\___/\___\___/|_| |___|"#;

const MIN_WIDE_COLUMNS: u16 = 84;
const MIN_COMPACT_COLUMNS: u16 = 62;
const MIN_SMALL_COLUMNS: u16 = 50;
const MIN_TEXT_COLUMNS: u16 = 30;
const MIN_MINIMAL_COLUMNS: u16 = 15;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BodyLayout {
    Full,
    Short,
    Narrow,
    Minimal,
}

pub(crate) fn print() {
    let version = env!("CARGO_PKG_VERSION");
    let lines = if std::io::stdout().is_terminal() {
        let width = terminal_columns().unwrap_or(80);
        banner_lines(width, version)
    } else {
        piped_lines(version)
    };
    for line in lines {
        outln!("{line}");
    }
}

/// Lines for the static banner at a known width.
pub(crate) fn banner_lines(width: u16, version: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let logo = logo_for_width(Some(width));
    if !logo.is_empty() {
        push_logo(&mut lines, logo);
    }
    lines.extend(body_lines(body_layout(width), version, width));
    lines
}

/// Piped output keeps the compact mark and the full body.
fn piped_lines(version: &str) -> Vec<String> {
    let mut lines = Vec::new();
    push_logo(&mut lines, COMPACT_LOGO.trim_start_matches('\n'));
    lines.extend(body_lines(BodyLayout::Full, version, 80));
    lines
}

fn push_logo(lines: &mut Vec<String>, logo: &str) {
    for line in logo.lines() {
        lines.push(format!("{ACCENT}{line}{ACCENT:#}"));
    }
    lines.push(String::new());
}

fn body_layout(width: u16) -> BodyLayout {
    if width >= MIN_WIDE_COLUMNS {
        BodyLayout::Full
    } else if width >= MIN_SMALL_COLUMNS {
        BodyLayout::Short
    } else if width >= MIN_TEXT_COLUMNS {
        BodyLayout::Narrow
    } else {
        BodyLayout::Minimal
    }
}

fn body_lines(layout: BodyLayout, version: &str, width: u16) -> Vec<String> {
    match layout {
        BodyLayout::Full => vec![
            format!(
                "  {BOLD}Blockscope{BOLD:#} {MUTED}v{version}{MUTED:#} - headless Litematic image renderer"
            ),
            String::new(),
            format!("  {BOLD}Quick start{BOLD:#}"),
            format!("    {MUTED}${MUTED:#} blockscope render model.litematic --output out.png"),
            format!("    {MUTED}${MUTED:#} blockscope render model.litematic --terminal --no-file"),
            format!("    {MUTED}${MUTED:#} blockscope inspect model.litematic --json"),
            String::new(),
            format!("  {BOLD}Commands{BOLD:#}"),
            format!("    {ACCENT}render{ACCENT:#}     PNG or terminal graphics, multi-view"),
            format!("    {ACCENT}inspect{ACCENT:#}    Manifest, bounds, statistics, scene hash"),
            format!("    {ACCENT}serve{ACCENT:#}      Long-lived JSON worker over stdio"),
            String::new(),
            format!("  Run {ACCENT}blockscope help{ACCENT:#} for the full command reference."),
        ],
        BodyLayout::Short => vec![
            format!("  {BOLD}Blockscope{BOLD:#} {MUTED}v{version}{MUTED:#}"),
            format!("  {MUTED}headless Litematic image renderer{MUTED:#}"),
            String::new(),
            format!("  {BOLD}Quick start{BOLD:#}"),
            format!("    {MUTED}${MUTED:#} blockscope render <file> --output out.png"),
            format!("    {MUTED}${MUTED:#} blockscope render <file> --terminal"),
            format!("    {MUTED}${MUTED:#} blockscope inspect <file>"),
            String::new(),
            format!("  {BOLD}Commands{BOLD:#}"),
            format!(
                "    {ACCENT}render{ACCENT:#}  {ACCENT}inspect{ACCENT:#}  {ACCENT}serve{ACCENT:#}"
            ),
            String::new(),
            format!("  Run {ACCENT}blockscope help{ACCENT:#}"),
        ],
        BodyLayout::Narrow => vec![
            format!("  {BOLD}BLOCKSCOPE{BOLD:#} {MUTED}v{version}{MUTED:#}"),
            format!("  {MUTED}headless Litematic renderer{MUTED:#}"),
            String::new(),
            format!("  {ACCENT}blockscope render <file>{ACCENT:#}"),
            format!("  {ACCENT}blockscope inspect <file>{ACCENT:#}"),
            format!("  {ACCENT}blockscope help{ACCENT:#}"),
        ],
        BodyLayout::Minimal => {
            if width >= 20 {
                vec![
                    format!("  {BOLD}Blockscope{BOLD:#} {MUTED}v{version}{MUTED:#}"),
                    format!("  {ACCENT}blockscope help{ACCENT:#}"),
                ]
            } else if width >= MIN_MINIMAL_COLUMNS {
                vec![
                    format!("{BOLD}Blockscope{BOLD:#}"),
                    format!("{ACCENT}blockscope help{ACCENT:#}"),
                ]
            } else {
                vec![format!("{ACCENT}blockscope{ACCENT:#}")]
            }
        }
    }
}

fn logo_for_width(columns: Option<u16>) -> &'static str {
    match columns {
        None => COMPACT_LOGO.trim_start_matches('\n'),
        Some(columns) if columns >= MIN_WIDE_COLUMNS => WIDE_LOGO,
        Some(columns) if columns >= MIN_COMPACT_COLUMNS => COMPACT_LOGO.trim_start_matches('\n'),
        Some(columns) if columns >= MIN_SMALL_COLUMNS => SMALL_LOGO.trim_start_matches('\n'),
        Some(_) => "",
    }
}

fn terminal_columns() -> Option<u16> {
    if let Some((_, columns)) = console::Term::stdout().size_checked()
        && columns > 0
    {
        return Some(columns);
    }
    if let Ok(value) = std::env::var("COLUMNS")
        && let Ok(columns) = value.parse::<u16>()
        && columns > 0
    {
        return Some(columns);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compact() -> &'static str {
        COMPACT_LOGO.trim_start_matches('\n')
    }

    fn small() -> &'static str {
        SMALL_LOGO.trim_start_matches('\n')
    }

    fn strip_ansi(text: &str) -> String {
        let mut out = String::new();
        let mut chars = text.chars();
        while let Some(ch) = chars.next() {
            if ch != '\u{1b}' {
                out.push(ch);
                continue;
            }
            if chars.next() == Some('[') {
                for ch in chars.by_ref() {
                    if ('@'..='~').contains(&ch) {
                        break;
                    }
                }
            }
        }
        out
    }

    fn visible_width(line: &str) -> usize {
        strip_ansi(line).chars().count()
    }

    #[test]
    fn wide_logo_needs_room_and_is_the_user_selected_art() {
        assert_eq!(logo_for_width(Some(120)), WIDE_LOGO);
        assert_eq!(logo_for_width(Some(84)), WIDE_LOGO);
        assert!(WIDE_LOGO.starts_with("██████╗"));
    }

    #[test]
    fn wide_logo_spells_blockscope_not_blockecope() {
        // K's third row followed by S's third row, then C.
        assert!(WIDE_LOGO.contains("█████╔╝ ███████╗██║"));
        // The misspelling used E's third/fourth rows in place of the S.
        assert!(!WIDE_LOGO.contains("█████╔╝ █████╗  ██║"));
        assert!(WIDE_LOGO.contains("██╔═██╗ ╚════██║██║"));
    }

    #[test]
    fn logo_tiers_follow_terminal_width() {
        assert_eq!(logo_for_width(Some(83)), compact());
        assert_eq!(logo_for_width(Some(62)), compact());
        assert_eq!(logo_for_width(Some(61)), small());
        assert_eq!(logo_for_width(Some(50)), small());
        assert_eq!(logo_for_width(Some(49)), "");
        assert_eq!(logo_for_width(Some(20)), "");
        assert_eq!(logo_for_width(None), compact());
    }

    #[test]
    fn body_gets_shorter_as_the_terminal_narrows() {
        assert_eq!(body_layout(120), BodyLayout::Full);
        assert_eq!(body_layout(84), BodyLayout::Full);
        assert_eq!(body_layout(83), BodyLayout::Short);
        assert_eq!(body_layout(50), BodyLayout::Short);
        assert_eq!(body_layout(49), BodyLayout::Narrow);
        assert_eq!(body_layout(30), BodyLayout::Narrow);
        assert_eq!(body_layout(29), BodyLayout::Minimal);
        assert_eq!(body_layout(10), BodyLayout::Minimal);
    }

    #[test]
    fn banner_lines_never_exceed_the_width() {
        for width in [120, 84, 83, 62, 61, 50, 49, 40, 30, 29, 20, 18, 15] {
            let lines = banner_lines(width, "0.0.0");
            assert!(
                lines
                    .iter()
                    .all(|line| visible_width(line) <= usize::from(width)),
                "width {width} overflowed: {lines:?}"
            );
        }
    }

    #[test]
    fn logo_widths_fit_their_tier() {
        assert!(WIDE_LOGO.lines().all(|line| line.chars().count() <= 82));
        assert!(compact().lines().all(|line| line.chars().count() <= 59));
        assert!(small().lines().all(|line| line.chars().count() <= 47));
    }
}
