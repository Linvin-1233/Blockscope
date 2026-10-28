//! Terminal presentation: color resolution, style tokens, and TTY-aware output.

pub(crate) mod banner;
mod console;
pub(crate) mod progress;

pub(crate) use console::ConsoleGuard;

use anstream::{AutoStream, ColorChoice as StreamColorChoice};
use anstyle::{AnsiColor, Effects, Style};
use clap::ValueEnum;
use std::fmt::Arguments;
use std::io::{self, Write};
use std::sync::atomic::{AtomicU8, Ordering};

/// When to emit ANSI escape sequences.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub(crate) enum ColorMode {
    /// Color only on a terminal, honoring `NO_COLOR` and `CLICOLOR_FORCE`.
    #[default]
    Auto,
    /// Always emit color, even through a pipe.
    Always,
    /// Never emit color.
    Never,
}

impl ColorMode {
    pub(crate) const fn clap_choice(self) -> clap::ColorChoice {
        match self {
            Self::Auto => clap::ColorChoice::Auto,
            Self::Always => clap::ColorChoice::Always,
            Self::Never => clap::ColorChoice::Never,
        }
    }

    const fn stream_choice(self) -> StreamColorChoice {
        match self {
            Self::Auto => StreamColorChoice::Auto,
            Self::Always => StreamColorChoice::Always,
            Self::Never => StreamColorChoice::Never,
        }
    }

    const fn as_u8(self) -> u8 {
        match self {
            Self::Auto => 0,
            Self::Always => 1,
            Self::Never => 2,
        }
    }

    const fn from_u8(value: u8) -> Self {
        match value {
            1 => Self::Always,
            2 => Self::Never,
            _ => Self::Auto,
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "auto" => Some(Self::Auto),
            "always" => Some(Self::Always),
            "never" => Some(Self::Never),
            _ => None,
        }
    }
}

static COLOR_MODE: AtomicU8 = AtomicU8::new(0);

/// Record the resolved color mode for all later writes.
pub(crate) fn init(mode: ColorMode) {
    COLOR_MODE.store(mode.as_u8(), Ordering::Relaxed);
}

pub(crate) fn color_mode() -> ColorMode {
    ColorMode::from_u8(COLOR_MODE.load(Ordering::Relaxed))
}

/// Scan argv before clap runs so help and parse errors use the requested mode.
pub(crate) fn color_mode_from_args() -> ColorMode {
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        let Some(text) = arg.to_str() else {
            continue;
        };
        if text == "--color" {
            if let Some(mode) = args
                .next()
                .and_then(|value| value.to_str().and_then(ColorMode::parse))
            {
                return mode;
            }
        } else if let Some(value) = text.strip_prefix("--color=")
            && let Some(mode) = ColorMode::parse(value)
        {
            return mode;
        }
    }
    ColorMode::Auto
}

pub(crate) fn clap_styles() -> clap::builder::styling::Styles {
    use clap::builder::styling::{AnsiColor as ClapColor, Effects as ClapEffects, Styles};

    Styles::styled()
        .header(ClapColor::Cyan.on_default().effects(ClapEffects::BOLD))
        .usage(ClapColor::Cyan.on_default().effects(ClapEffects::BOLD))
        .literal(ClapColor::Green.on_default().effects(ClapEffects::BOLD))
        .placeholder(ClapColor::BrightBlack.on_default())
        .error(ClapColor::Red.on_default().effects(ClapEffects::BOLD))
        .valid(ClapColor::Green.on_default())
        .invalid(ClapColor::Yellow.on_default())
}

pub(crate) const ACCENT: Style = AnsiColor::Cyan.on_default().effects(Effects::BOLD);
pub(crate) const SUCCESS: Style = AnsiColor::Green.on_default().effects(Effects::BOLD);
pub(crate) const WARNING: Style = AnsiColor::Yellow.on_default().effects(Effects::BOLD);
pub(crate) const ERROR: Style = AnsiColor::Red.on_default().effects(Effects::BOLD);
pub(crate) const MUTED: Style = AnsiColor::BrightBlack.on_default();
pub(crate) const BOLD: Style = Style::new().effects(Effects::BOLD);
pub(crate) const HEADER: Style = AnsiColor::Cyan.on_default().effects(Effects::BOLD);

pub(crate) fn write_out(args: Arguments<'_>) {
    let mut out = AutoStream::new(io::stdout(), color_mode().stream_choice());
    let _ = writeln!(out, "{args}");
}

pub(crate) fn write_err(args: Arguments<'_>) {
    let mut err = AutoStream::new(io::stderr(), color_mode().stream_choice());
    let _ = writeln!(err, "{args}");
}

macro_rules! outln {
    () => {
        $crate::ui::write_out(format_args!(""))
    };
    ($($arg:tt)*) => {
        $crate::ui::write_out(format_args!($($arg)*))
    };
}

macro_rules! errln {
    () => {
        $crate::ui::write_err(format_args!(""))
    };
    ($($arg:tt)*) => {
        $crate::ui::write_err(format_args!($($arg)*))
    };
}

pub(crate) use {errln, outln};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_mode_parses_documented_values() {
        assert_eq!(ColorMode::parse("auto"), Some(ColorMode::Auto));
        assert_eq!(ColorMode::parse("always"), Some(ColorMode::Always));
        assert_eq!(ColorMode::parse("never"), Some(ColorMode::Never));
        assert_eq!(ColorMode::parse("sometimes"), None);
    }

    #[test]
    fn color_mode_round_trips_through_the_atomic_representation() {
        for mode in [ColorMode::Auto, ColorMode::Always, ColorMode::Never] {
            assert_eq!(ColorMode::from_u8(mode.as_u8()), mode);
        }
    }
}
