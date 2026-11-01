use std::collections::HashMap;

use crate::error::{Error, Result};

// Parsed command line: `--name value` / `--name=value` options, bare
// positionals, and everything after `--` forwarded verbatim.
pub struct Args {
    positionals: Vec<String>,
    options: HashMap<String, String>,
    pub rest: Vec<String>,
}

impl Args {
    pub fn parse(argv: Vec<String>) -> Result<Self> {
        let mut positionals = Vec::new();
        let mut options = HashMap::new();
        let mut rest = Vec::new();
        let mut index = 0;

        while index < argv.len() {
            let arg = &argv[index];
            index += 1;

            if arg == "--" {
                rest.extend_from_slice(&argv[index..]);
                break;
            }

            let Some(option) = arg.strip_prefix("--") else {
                positionals.push(arg.clone());
                continue;
            };

            if let Some((name, value)) = option.split_once('=') {
                options.insert(name.to_owned(), value.to_owned());
                continue;
            }

            match argv.get(index) {
                Some(next) if !next.starts_with("--") => {
                    options.insert(option.to_owned(), next.clone());
                    index += 1;
                }
                _ => {
                    options.insert(option.to_owned(), String::new());
                }
            }
        }

        Ok(Self {
            positionals,
            options,
            rest,
        })
    }

    pub fn positional(&self, index: usize) -> Option<&str> {
        self.positionals.get(index).map(String::as_str)
    }

    pub fn require_positional(&self, index: usize, usage: &str) -> Result<&str> {
        self.positional(index).ok_or_else(|| Error::message(usage))
    }

    pub fn option(&self, name: &str) -> Option<&str> {
        self.options
            .get(name)
            .map(String::as_str)
            .filter(|value| !value.is_empty())
    }

    pub fn require_option(&self, name: &str) -> Result<&str> {
        self.option(name)
            .ok_or_else(|| Error::message(format!("missing --{name}")))
    }
}

#[cfg(test)]
mod tests {
    use super::Args;

    fn parse(argv: &[&str]) -> Args {
        Args::parse(argv.iter().map(|arg| (*arg).to_owned()).collect()).unwrap()
    }

    #[test]
    fn separates_options_positionals_and_forwarded_args() {
        let args = parse(&[
            "litematic",
            "--path",
            "/fixtures",
            "--",
            "--nocapture",
            "--exact",
        ]);

        assert_eq!(args.positional(0), Some("litematic"));
        assert_eq!(args.option("path"), Some("/fixtures"));
        assert_eq!(args.rest, ["--nocapture", "--exact"]);
    }

    #[test]
    fn accepts_equals_form_and_empty_values() {
        let args = parse(&["--tag=v1.2.3", "--version", ""]);

        assert_eq!(args.option("tag"), Some("v1.2.3"));
        // An empty value behaves like an absent option so callers can fall back
        // to their defaults.
        assert_eq!(args.option("version"), None);
    }

    #[test]
    fn does_not_take_the_next_option_as_a_value() {
        let args = parse(&["--msi", "--verbose"]);

        assert_eq!(args.option("msi"), None);
        assert_eq!(args.positional(0), None);
    }
}
