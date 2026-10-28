use super::protocol::TerminalProtocol;
use std::env;

/// Enable virtual terminal processing on Windows console so ANSI escape sequences render properly.
///
/// On Windows 10/11 PowerShell and Command Prompt (`conhost.exe`), virtual terminal processing
/// must be enabled on `STD_OUTPUT_HANDLE` to parse ANSI colors and Unicode characters.
pub fn enable_windows_virtual_terminal() {
    #[cfg(windows)]
    {
        const STD_OUTPUT_HANDLE: u32 = 0xFFFFFFF5; // -11i32 as u32
        const ENABLE_VIRTUAL_TERMINAL_PROCESSING: u32 = 0x0004;

        // SAFETY: the signatures match the Win32 console exports.
        unsafe extern "system" {
            fn GetStdHandle(nStdHandle: u32) -> *mut std::ffi::c_void;
            fn GetConsoleMode(hConsoleHandle: *mut std::ffi::c_void, lpMode: *mut u32) -> i32;
            fn SetConsoleMode(hConsoleHandle: *mut std::ffi::c_void, dwMode: u32) -> i32;
        }

        // SAFETY: the handle is checked for null/INVALID_HANDLE_VALUE, and
        // `mode` is a live u32 buffer for `GetConsoleMode`.
        unsafe {
            let handle = GetStdHandle(STD_OUTPUT_HANDLE);
            if !handle.is_null() && handle as isize != -1 {
                let mut mode: u32 = 0;
                if GetConsoleMode(handle, &mut mode) != 0 {
                    let _ = SetConsoleMode(handle, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING);
                }
            }
        }
    }
}

/// Detect the best supported terminal image protocol from the current environment.
pub fn detect_terminal_protocol() -> TerminalProtocol {
    // 1. Kitty Graphics Protocol
    // Kitty sets KITTY_WINDOW_ID / KITTY_PID; Ghostty and foot also support Kitty graphics.
    if env::var_os("KITTY_WINDOW_ID").is_some() || env::var_os("KITTY_PID").is_some() {
        return TerminalProtocol::Kitty;
    }
    if env::var_os("GHOSTTY_RESOURCES_DIR").is_some() {
        return TerminalProtocol::Kitty;
    }
    if let Ok(term) = env::var("TERM")
        && (term == "xterm-kitty" || term == "xterm-ghostty")
    {
        return TerminalProtocol::Kitty;
    }

    // 2. iTerm2 Inline Image Protocol
    // Supported by iTerm2, WezTerm, VS Code terminal (including on Windows/PowerShell), Warp, mintty.
    if let Ok(program) = env::var("TERM_PROGRAM") {
        let p = program.to_ascii_lowercase();
        if p.contains("iterm")
            || p.contains("wezterm")
            || p.contains("vscode")
            || p.contains("warp")
            || p.contains("mintty")
        {
            return TerminalProtocol::Iterm2;
        }
    }
    if let Ok(lc_term) = env::var("LC_TERMINAL")
        && lc_term.to_ascii_lowercase().contains("iterm")
    {
        return TerminalProtocol::Iterm2;
    }

    // 3. Sixel Graphics Protocol
    // Supported by mlterm, foot, xterm with sixel enabled, etc.
    if let Ok(term) = env::var("TERM") {
        let t = term.to_ascii_lowercase();
        if t.contains("sixel") || t.starts_with("mlterm") || t == "foot" {
            return TerminalProtocol::Sixel;
        }
    }

    // 4. Windows Terminal (`WT_SESSION`) or generic console (PowerShell / conhost / Linux console)
    // Halfblocks (Unicode ▀ + ANSI 24-bit Truecolor) is universally supported.
    TerminalProtocol::Halfblocks
}

/// Detect the current terminal width and height in character cells (columns, rows).
pub fn detect_terminal_size() -> Option<(u16, u16)> {
    #[cfg(unix)]
    {
        #[repr(C)]
        struct WinSize {
            ws_row: u16,
            ws_col: u16,
            ws_xpixel: u16,
            ws_ypixel: u16,
        }

        #[cfg(target_os = "macos")]
        const TIOCGWINSZ: std::ffi::c_ulong = 0x40087468;
        #[cfg(not(target_os = "macos"))]
        const TIOCGWINSZ: std::ffi::c_ulong = 0x5413;

        // SAFETY: the variadic signature matches the platform libc `ioctl`.
        unsafe extern "C" {
            fn ioctl(fd: i32, request: std::ffi::c_ulong, ...) -> i32;
        }

        // SAFETY: `WinSize` is a plain C-layout struct, and `ioctl` writes a
        // `winsize` into the live buffer when `TIOCGWINSZ` succeeds.
        unsafe {
            let mut ws: WinSize = std::mem::zeroed();
            // 1 is STDOUT_FILENO
            if ioctl(1, TIOCGWINSZ, &mut ws) == 0 && ws.ws_col > 0 && ws.ws_row > 0 {
                return Some((ws.ws_col, ws.ws_row));
            }
        }
    }
    #[cfg(windows)]
    {
        const STD_OUTPUT_HANDLE: u32 = 0xFFFFFFF5;

        #[repr(C)]
        struct Coord {
            x: i16,
            y: i16,
        }

        #[repr(C)]
        struct SmallRect {
            left: i16,
            top: i16,
            right: i16,
            bottom: i16,
        }

        #[repr(C)]
        struct ConsoleScreenBufferInfo {
            dw_size: Coord,
            dw_cursor_position: Coord,
            w_attributes: u16,
            sr_window: SmallRect,
            dw_maximum_window_size: Coord,
        }

        // SAFETY: the signatures match the Win32 console exports.
        unsafe extern "system" {
            fn GetStdHandle(nStdHandle: u32) -> *mut std::ffi::c_void;
            fn GetConsoleScreenBufferInfo(
                hConsoleOutput: *mut std::ffi::c_void,
                lpConsoleScreenBufferInfo: *mut ConsoleScreenBufferInfo,
            ) -> i32;
        }

        // SAFETY: the handle is checked for null/INVALID_HANDLE_VALUE, and
        // `csbi` is a live `ConsoleScreenBufferInfo` buffer.
        unsafe {
            let handle = GetStdHandle(STD_OUTPUT_HANDLE);
            if !handle.is_null() && handle as isize != -1 {
                let mut csbi: ConsoleScreenBufferInfo = std::mem::zeroed();
                if GetConsoleScreenBufferInfo(handle, &mut csbi) != 0 {
                    let width = (csbi.sr_window.right - csbi.sr_window.left + 1).max(0) as u16;
                    let height = (csbi.sr_window.bottom - csbi.sr_window.top + 1).max(0) as u16;
                    if width > 0 && height > 0 {
                        return Some((width, height));
                    }
                }
            }
        }
    }

    // Fallback to environment variables (e.g. COLUMNS / LINES set in PowerShell or bash)
    let cols = env::var("COLUMNS").ok().and_then(|s| s.parse::<u16>().ok());
    let rows = env::var("LINES").ok().and_then(|s| s.parse::<u16>().ok());

    match (cols, rows) {
        (Some(c), Some(r)) if c > 0 && r > 0 => Some((c, r)),
        (Some(c), None) if c > 0 => Some((c, 24)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_protocol_returns_valid_protocol() {
        let protocol = detect_terminal_protocol();
        assert!(TerminalProtocol::ALL.contains(&protocol));
    }

    #[test]
    fn enable_vt_mode_does_not_panic() {
        enable_windows_virtual_terminal();
    }
}
