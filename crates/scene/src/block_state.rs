use std::collections::BTreeMap;
use std::fmt;

/// A canonical block state key independent of Minecraft's numeric IDs.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlockStateKey {
    pub name: String,
    pub properties: BTreeMap<String, String>,
}

impl BlockStateKey {
    pub fn new(name: impl Into<String>, properties: BTreeMap<String, String>) -> Self {
        Self {
            name: name.into(),
            properties,
        }
    }

    pub fn simple(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            properties: BTreeMap::new(),
        }
    }

    pub fn is_air(&self) -> bool {
        matches!(
            self.name.as_str(),
            "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air"
        )
    }

    /// Deterministic textual form used by manifests and diagnostics.
    pub fn canonical(&self) -> String {
        if self.properties.is_empty() {
            return self.name.clone();
        }

        let properties = self
            .properties
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join(",");

        format!("{}[{properties}]", self.name)
    }
}

impl fmt::Display for BlockStateKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.canonical())
    }
}

impl From<&str> for BlockStateKey {
    fn from(name: &str) -> Self {
        Self::simple(name)
    }
}
