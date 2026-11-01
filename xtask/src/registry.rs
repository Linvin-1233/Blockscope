// Reader for `reg query` output, used by the MSI lifecycle check. Keeping the
// parsing pure lets the format be tested without a Windows registry.

use std::collections::BTreeMap;
use std::process::Command;

use crate::error::Result;
use crate::process::output;

pub struct Key {
    values: BTreeMap<String, String>,
}

impl Key {
    pub fn value(&self, name: &str) -> Option<&str> {
        self.values.get(name).map(String::as_str)
    }
}

// Subkeys of `root` with their values. A missing key is reported by reg.exe as
// a failure, which callers treat as "not installed".
pub fn query(root: &str) -> Result<Vec<Key>> {
    let output = output(Command::new("reg").args(["query", root, "/s"]))?;
    if !output.status.success() {
        return Ok(Vec::new());
    }
    Ok(parse(&String::from_utf8_lossy(&output.stdout)))
}

fn parse(text: &str) -> Vec<Key> {
    let mut keys: Vec<Key> = Vec::new();

    for line in text.lines() {
        // reg.exe terminates lines with CRLF, and carriage returns would end up
        // inside the parsed values.
        let line = line.trim_end_matches('\r');

        if line.starts_with("HKEY_") {
            keys.push(Key {
                values: BTreeMap::new(),
            });
            continue;
        }

        let Some((name, value)) = parse_value_line(line) else {
            continue;
        };
        if let Some(key) = keys.last_mut() {
            key.values.insert(name, value);
        }
    }

    keys
}

// Value lines are indented and use runs of spaces as column separators:
// `    DisplayName    REG_SZ    Blockscope`.
fn parse_value_line(line: &str) -> Option<(String, String)> {
    let indented = line.len() - line.trim_start_matches(' ').len();
    if indented < 2 {
        return None;
    }

    let (name, rest) = split_columns(&line[indented..])?;
    let (kind, value) = match split_columns(rest) {
        Some((kind, value)) => (kind, value),
        None => (rest, ""),
    };
    if !kind.starts_with("REG_") {
        return None;
    }

    Some((name.to_owned(), value.to_owned()))
}

// Splits at the first run of two or more spaces.
fn split_columns(text: &str) -> Option<(&str, &str)> {
    let bytes = text.as_bytes();
    for index in 0..bytes.len().saturating_sub(1) {
        if bytes[index] == b' ' && bytes[index + 1] == b' ' {
            return Some((&text[..index], text[index..].trim_start_matches(' ')));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::parse;

    const SAMPLE: &str = concat!(
        "\r\n",
        "HKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{ABC}\r\n",
        "    DisplayName    REG_SZ    Blockscope\r\n",
        "    InstallLocation    REG_SZ    C:\\Program Files\\Blockscope\r\n",
        "\r\n",
        "HKEY_CURRENT_USER\\Environment\r\n",
        "    Path    REG_EXPAND_SZ    %USERPROFILE%\\bin;;C:\\Tools\\\r\n",
    );

    #[test]
    fn groups_values_under_their_key() {
        let keys = parse(SAMPLE);

        assert_eq!(keys.len(), 2);
        assert_eq!(keys[0].value("DisplayName"), Some("Blockscope"));
        assert_eq!(
            keys[0].value("InstallLocation"),
            Some("C:\\Program Files\\Blockscope")
        );
        assert_eq!(
            keys[1].value("Path"),
            Some("%USERPROFILE%\\bin;;C:\\Tools\\")
        );
    }

    #[test]
    fn keeps_empty_values_and_ignores_noise() {
        let text = "HKEY_CURRENT_USER\\Software\\Thing\r\n    Empty    REG_SZ\r\n\
                    End of search: 1 match(es) found.\r\n";

        let keys = parse(text);

        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].value("Empty"), Some(""));
    }
}
