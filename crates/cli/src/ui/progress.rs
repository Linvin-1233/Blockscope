//! Progress feedback for long-running commands.
//!
//! Everything here is opt-in: progress is hidden when stderr is not a terminal,
//! when the command emits JSON, or when the user asks for no styling.

use crate::ui::ColorMode;
use indicatif::{ProgressBar, ProgressStyle};
use std::borrow::Cow;
use std::io::IsTerminal;
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::Duration;

const TICK_CHARS: &str = "-\\|/ ";
const STEADY_TICK: Duration = Duration::from_millis(120);
const REFLOW_INTERVAL: Duration = Duration::from_millis(200);
const SPINNER_TEMPLATE: &str = "{spinner} {msg} ({elapsed})";
const SPINNER_TEMPLATE_NARROW: &str = "{spinner} {msg}";
const BAR_TEMPLATE: &str = "{msg} [{bar:32}] {pos}/{len}";
const BAR_TEMPLATE_MEDIUM: &str = "{msg} [{bar:16}] {pos}/{len}";
const BAR_TEMPLATE_NARROW: &str = "{msg} {pos}/{len}";
const BAR_TEMPLATE_TINY: &str = "{pos}/{len}";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WidgetKind {
    Spinner,
    Bar,
}

impl WidgetKind {
    /// Template plus the columns reserved for everything except the message.
    const fn template(self, width: u16) -> (&'static str, u16) {
        match self {
            Self::Spinner => {
                if width < 60 {
                    (SPINNER_TEMPLATE_NARROW, 14)
                } else {
                    (SPINNER_TEMPLATE, 14)
                }
            }
            Self::Bar => match width {
                90.. => (BAR_TEMPLATE, 48),
                60..=89 => (BAR_TEMPLATE_MEDIUM, 32),
                40..=59 => (BAR_TEMPLATE_NARROW, 14),
                _ => (BAR_TEMPLATE_TINY, 0),
            },
        }
    }

    fn style(self, width: u16) -> ProgressStyle {
        let (template, _) = self.template(width);
        let style = ProgressStyle::with_template(template).unwrap_or_else(|_| match self {
            Self::Spinner => ProgressStyle::default_spinner(),
            Self::Bar => ProgressStyle::default_bar(),
        });
        match self {
            Self::Spinner => style.tick_chars(TICK_CHARS),
            Self::Bar => style.progress_chars("█▉▊▋▌▍▎▏  "),
        }
    }
}

/// Creates hidden or visible progress widgets for one command invocation.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Progress {
    enabled: bool,
}

impl Progress {
    pub(crate) fn new(json: bool) -> Self {
        let term = std::env::var("TERM").ok();
        let ci = std::env::var_os("CI").is_some();
        let mode = crate::ui::color_mode();
        let no_color = mode != ColorMode::Always && std::env::var_os("NO_COLOR").is_some();
        let enabled = progress_enabled(
            json,
            term.as_deref(),
            ci,
            no_color,
            std::io::stderr().is_terminal(),
            mode,
        );
        if enabled {
            start_watcher();
        }
        Self { enabled }
    }

    /// An indeterminate stage with elapsed time.
    pub(crate) fn stage(&self, message: &'static str) -> Stage {
        if !self.enabled {
            return Stage { state: None };
        }
        let width = stderr_width().unwrap_or(80);
        let bar = ProgressBar::new_spinner();
        bar.set_style(WidgetKind::Spinner.style(width));
        bar.set_message(fit(message, message_width(WidgetKind::Spinner, width)));
        bar.enable_steady_tick(STEADY_TICK);
        let state = WidgetState::new(bar, message, WidgetKind::Spinner, width);
        register(&state);
        Stage { state: Some(state) }
    }

    /// A determinate bar. The length may be supplied by the first update.
    pub(crate) fn bar(&self, message: &'static str) -> Bar {
        if !self.enabled {
            return Bar {
                state: None,
                last: Mutex::new(0),
            };
        }
        let width = stderr_width().unwrap_or(80);
        let bar = ProgressBar::new(0);
        bar.set_style(WidgetKind::Bar.style(width));
        bar.set_message(fit(message, message_width(WidgetKind::Bar, width)));
        let state = WidgetState::new(bar, message, WidgetKind::Bar, width);
        register(&state);
        Bar {
            state: Some(state),
            last: Mutex::new(0),
        }
    }
}

struct WidgetState {
    bar: ProgressBar,
    message: Mutex<Cow<'static, str>>,
    kind: WidgetKind,
    width: AtomicU16,
    finished: AtomicBool,
    lifecycle: Mutex<()>,
}

impl WidgetState {
    fn new(bar: ProgressBar, message: &'static str, kind: WidgetKind, width: u16) -> Arc<Self> {
        Arc::new(Self {
            bar,
            message: Mutex::new(Cow::Borrowed(message)),
            kind,
            width: AtomicU16::new(width),
            finished: AtomicBool::new(false),
            lifecycle: Mutex::new(()),
        })
    }

    fn reflow(&self, width: u16) {
        let _lifecycle = self
            .lifecycle
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if self.finished.load(Ordering::Relaxed) || width == 0 {
            return;
        }
        self.width.store(width, Ordering::Relaxed);
        self.bar.set_style(self.kind.style(width));
        let message = self
            .message
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        self.bar
            .set_message(fit(message, message_width(self.kind, width)));
    }

    fn finish(&self) {
        let _lifecycle = self
            .lifecycle
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        self.finished.store(true, Ordering::Relaxed);
        self.bar.finish_and_clear();
    }
}

pub(crate) struct Stage {
    state: Option<Arc<WidgetState>>,
}

impl Stage {
    pub(crate) fn message(&self, message: impl Into<Cow<'static, str>>) {
        if let Some(state) = &self.state {
            *state
                .message
                .lock()
                .unwrap_or_else(|error| error.into_inner()) = message.into();
            state.reflow(state.width.load(Ordering::Relaxed));
        }
    }

    pub(crate) fn finish(mut self) {
        self.clear();
    }

    fn clear(&mut self) {
        if let Some(state) = self.state.take() {
            state.finish();
        }
    }
}

impl Drop for Stage {
    fn drop(&mut self) {
        self.clear();
    }
}

pub(crate) struct Bar {
    state: Option<Arc<WidgetState>>,
    last: Mutex<u64>,
}

impl Bar {
    pub(crate) fn update(&self, done: u64, total: u64) {
        let Some(state) = &self.state else {
            return;
        };
        let done = done.min(total);
        let mut last = self.last.lock().unwrap_or_else(|error| error.into_inner());
        if done > *last {
            if state.bar.length() != Some(total) {
                state.bar.set_length(total);
            }
            state.bar.set_position(done);
            *last = done;
        }
    }

    pub(crate) fn message(&self, message: impl Into<Cow<'static, str>>) {
        if let Some(state) = &self.state {
            *state
                .message
                .lock()
                .unwrap_or_else(|error| error.into_inner()) = message.into();
            state.reflow(state.width.load(Ordering::Relaxed));
        }
    }

    pub(crate) fn finish(mut self) {
        self.clear();
    }

    fn clear(&mut self) {
        if let Some(state) = self.state.take() {
            state.finish();
        }
    }
}

impl Drop for Bar {
    fn drop(&mut self) {
        self.clear();
    }
}

fn message_width(kind: WidgetKind, width: u16) -> usize {
    let (_, reserved) = kind.template(width);
    usize::from(width.saturating_sub(reserved))
}

fn stderr_width() -> Option<u16> {
    console::Term::stderr()
        .size_checked()
        .map(|(_, columns)| columns)
        .filter(|&columns| columns > 0)
}

/// Trim a message so the spinner or bar does not wrap on a narrow terminal.
fn fit(message: impl Into<Cow<'static, str>>, width: usize) -> Cow<'static, str> {
    let message = message.into();
    if message.chars().count() <= width {
        return message;
    }
    let mut text: String = message.chars().take(width.saturating_sub(3)).collect();
    text.push_str("...");
    Cow::Owned(text)
}

fn registry() -> &'static Mutex<Vec<Weak<WidgetState>>> {
    static REGISTRY: OnceLock<Mutex<Vec<Weak<WidgetState>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(Vec::new()))
}

fn register(state: &Arc<WidgetState>) {
    let mut registry = registry().lock().unwrap_or_else(|error| error.into_inner());
    registry.retain(|weak| weak.strong_count() > 0);
    registry.push(Arc::downgrade(state));
}

/// Watch stderr for resize events and reflow every live widget.
fn start_watcher() {
    static STARTED: OnceLock<()> = OnceLock::new();
    STARTED.get_or_init(|| {
        std::thread::spawn(|| {
            let mut last_width = 0;
            loop {
                std::thread::sleep(REFLOW_INTERVAL);
                let Some(width) = stderr_width() else {
                    continue;
                };
                if width == last_width {
                    continue;
                }
                last_width = width;

                let states = {
                    let mut registry = registry().lock().unwrap_or_else(|error| error.into_inner());
                    registry.retain(|weak| weak.strong_count() > 0);
                    registry
                        .iter()
                        .filter_map(Weak::upgrade)
                        .collect::<Vec<_>>()
                };
                for state in states {
                    state.reflow(width);
                }
            }
        });
    });
}

fn progress_enabled(
    json: bool,
    term: Option<&str>,
    ci: bool,
    no_color: bool,
    stderr_is_terminal: bool,
    mode: ColorMode,
) -> bool {
    !json
        && term != Some("dumb")
        && !ci
        && !no_color
        && stderr_is_terminal
        && mode != ColorMode::Never
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_requires_a_terminal_and_human_output() {
        assert!(progress_enabled(
            false,
            Some("xterm-256color"),
            false,
            false,
            true,
            ColorMode::Auto
        ));
        assert!(!progress_enabled(
            false,
            Some("xterm-256color"),
            false,
            false,
            false,
            ColorMode::Auto
        ));
        assert!(!progress_enabled(
            true,
            Some("xterm-256color"),
            false,
            false,
            true,
            ColorMode::Auto
        ));
    }

    #[test]
    fn progress_respects_ci_dumb_terminals_no_color_and_color_never() {
        assert!(!progress_enabled(
            false,
            Some("xterm"),
            true,
            false,
            true,
            ColorMode::Auto
        ));
        assert!(!progress_enabled(
            false,
            Some("dumb"),
            false,
            false,
            true,
            ColorMode::Auto
        ));
        assert!(!progress_enabled(
            false,
            Some("xterm"),
            false,
            true,
            true,
            ColorMode::Auto
        ));
        assert!(!progress_enabled(
            false,
            Some("xterm"),
            false,
            false,
            true,
            ColorMode::Never
        ));
    }

    #[test]
    fn templates_are_valid_at_every_width_tier() {
        for width in [120, 90, 89, 60, 59, 40, 39, 20] {
            for kind in [WidgetKind::Spinner, WidgetKind::Bar] {
                let (template, _) = kind.template(width);
                assert!(
                    ProgressStyle::with_template(template).is_ok(),
                    "width {width}: {template}"
                );
            }
        }
    }

    #[test]
    fn long_messages_are_trimmed_to_the_terminal_width() {
        assert_eq!(fit("short", 10), "short");
        let trimmed = fit("a very long progress message", 12);
        assert_eq!(trimmed.chars().count(), 12);
        assert!(trimmed.ends_with("..."));
    }
}
