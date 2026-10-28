//! Windows console preparation for the Unicode wordmark.

#[cfg(windows)]
use std::io::IsTerminal;

/// Switches an attached Windows console to UTF-8 and restores it on drop.
///
/// Rust writes UTF-8 bytes to stdout. A console using a legacy code page (for
/// example 936) interprets those bytes with a different encoding and garbles
/// block-drawing characters, so enable UTF-8 only while the process runs.
#[derive(Debug)]
pub(crate) struct ConsoleGuard {
    #[cfg(windows)]
    restore: Option<(u32, u32)>,
}

impl ConsoleGuard {
    pub(crate) fn enable_utf8() -> Self {
        #[cfg(windows)]
        {
            if !std::io::stdout().is_terminal() && !std::io::stderr().is_terminal() {
                return Self { restore: None };
            }

            use windows_sys::Win32::System::Console::{
                GetConsoleCP, GetConsoleOutputCP, SetConsoleCP, SetConsoleOutputCP,
            };

            // SAFETY: both calls only read console state for the current process.
            let (input, output) = unsafe { (GetConsoleCP(), GetConsoleOutputCP()) };
            if input == 0 || output == 0 {
                return Self { restore: None };
            }
            if input == 65001 && output == 65001 {
                return Self { restore: None };
            }

            // SAFETY: the code page ids come from the console API above.
            unsafe {
                SetConsoleCP(65001);
                SetConsoleOutputCP(65001);
            }
            Self {
                restore: Some((input, output)),
            }
        }
        #[cfg(not(windows))]
        {
            Self {}
        }
    }
}

#[cfg(windows)]
impl Drop for ConsoleGuard {
    fn drop(&mut self) {
        if let Some((input, output)) = self.restore {
            use windows_sys::Win32::System::Console::{SetConsoleCP, SetConsoleOutputCP};

            // SAFETY: restoring the values previously returned by the same API.
            unsafe {
                SetConsoleCP(input);
                SetConsoleOutputCP(output);
            }
        }
    }
}
