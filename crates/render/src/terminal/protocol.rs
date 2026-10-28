use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Supported terminal image rendering protocols.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TerminalProtocol {
    /// Kitty graphics protocol (APC escape sequences with chunked PNG transmission).
    Kitty,
    /// iTerm2 inline image protocol (OSC 1337 File transfer escape sequences).
    Iterm2,
    /// DEC Sixel graphics protocol (DCS escape sequences with palette quantization).
    Sixel,
    /// Universal half-block (▀) text fallback using ANSI 24-bit Truecolor.
    Halfblocks,
}

impl TerminalProtocol {
    /// All available protocols in priority order.
    pub const ALL: &'static [TerminalProtocol] = &[
        TerminalProtocol::Kitty,
        TerminalProtocol::Iterm2,
        TerminalProtocol::Sixel,
        TerminalProtocol::Halfblocks,
    ];

    /// Parse protocol name from string (case-insensitive with common aliases).
    pub fn parse(value: &str) -> Option<Self> {
        let normalized = value.trim().to_ascii_lowercase();
        match normalized.as_str() {
            "kitty" => Some(TerminalProtocol::Kitty),
            "iterm" | "iterm2" => Some(TerminalProtocol::Iterm2),
            "sixel" => Some(TerminalProtocol::Sixel),
            "halfblocks" | "halfblock" | "half-blocks" | "half-block" | "unicode" | "blocks"
            | "ansi" => Some(TerminalProtocol::Halfblocks),
            _ => None,
        }
    }

    /// Canonical identifier for the protocol.
    pub fn name(&self) -> &'static str {
        match self {
            TerminalProtocol::Kitty => "kitty",
            TerminalProtocol::Iterm2 => "iterm2",
            TerminalProtocol::Sixel => "sixel",
            TerminalProtocol::Halfblocks => "halfblocks",
        }
    }
}

impl fmt::Display for TerminalProtocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl FromStr for TerminalProtocol {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s).ok_or_else(|| {
            format!(
                "unknown terminal protocol '{s}', expected one of: kitty, iterm2, sixel, halfblocks"
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_recognizes_protocols_and_aliases() {
        assert_eq!(
            TerminalProtocol::parse("kitty"),
            Some(TerminalProtocol::Kitty)
        );
        assert_eq!(
            TerminalProtocol::parse("KITTY"),
            Some(TerminalProtocol::Kitty)
        );
        assert_eq!(
            TerminalProtocol::parse("iterm"),
            Some(TerminalProtocol::Iterm2)
        );
        assert_eq!(
            TerminalProtocol::parse("iterm2"),
            Some(TerminalProtocol::Iterm2)
        );
        assert_eq!(
            TerminalProtocol::parse("sixel"),
            Some(TerminalProtocol::Sixel)
        );
        assert_eq!(
            TerminalProtocol::parse("halfblocks"),
            Some(TerminalProtocol::Halfblocks)
        );
        assert_eq!(
            TerminalProtocol::parse("unicode"),
            Some(TerminalProtocol::Halfblocks)
        );
        assert_eq!(
            TerminalProtocol::parse("half-block"),
            Some(TerminalProtocol::Halfblocks)
        );
        assert_eq!(TerminalProtocol::parse("unknown"), None);
    }

    #[test]
    fn display_matches_canonical_name() {
        assert_eq!(TerminalProtocol::Kitty.to_string(), "kitty");
        assert_eq!(TerminalProtocol::Iterm2.to_string(), "iterm2");
        assert_eq!(TerminalProtocol::Sixel.to_string(), "sixel");
        assert_eq!(TerminalProtocol::Halfblocks.to_string(), "halfblocks");
    }

    #[test]
    fn from_str_round_trips() {
        for protocol in TerminalProtocol::ALL {
            let parsed: TerminalProtocol = protocol.name().parse().unwrap();
            assert_eq!(*protocol, parsed);
        }
    }
}
