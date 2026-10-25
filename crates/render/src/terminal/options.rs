use super::detect::detect_terminal_protocol;
use super::protocol::TerminalProtocol;

/// Configuration options for rendering images into terminal protocols.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalOptions {
    /// Terminal graphics protocol to use.
    pub protocol: TerminalProtocol,
    /// Explicit target width in character cells (columns).
    pub width: Option<u32>,
    /// Explicit target height in character cells (rows).
    pub height: Option<u32>,
    /// Explicit pixel width cap (for pixel-based protocols).
    pub pixel_width: Option<u32>,
    /// Explicit pixel height cap (for pixel-based protocols).
    pub pixel_height: Option<u32>,
    /// Preserve original aspect ratio when resizing (default true).
    pub preserve_aspect_ratio: bool,
    /// Suppress terminal protocol responses (e.g. Kitty quiet mode `q=2`, default true).
    pub quiet: bool,
}

impl Default for TerminalOptions {
    fn default() -> Self {
        Self {
            protocol: detect_terminal_protocol(),
            width: None,
            height: None,
            pixel_width: None,
            pixel_height: None,
            preserve_aspect_ratio: true,
            quiet: true,
        }
    }
}

impl TerminalOptions {
    /// Create options with a specific protocol.
    pub fn new(protocol: TerminalProtocol) -> Self {
        Self {
            protocol,
            ..Default::default()
        }
    }

    /// Set target dimensions in character cells.
    pub fn with_cells(mut self, width: Option<u32>, height: Option<u32>) -> Self {
        self.width = width;
        self.height = height;
        self
    }
}
