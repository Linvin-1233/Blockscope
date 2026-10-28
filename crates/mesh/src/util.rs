/// Remove a leading `namespace:` from a resource location.
pub(crate) fn strip_namespace(name: &str) -> &str {
    name.split_once(':').map(|(_, path)| path).unwrap_or(name)
}
