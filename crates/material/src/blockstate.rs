use crate::json::{BlockstateFile, Condition, Variant};
use litematic_scene::BlockStateKey;

/// Select the deterministic model variants for a block state.
///
/// Variant arrays are weighted/random in Minecraft; a static render uses the
/// first entry so repeated renders stay byte-identical.
pub fn select_variants(file: &BlockstateFile, state: &BlockStateKey) -> Vec<Variant> {
    if !file.variants.is_empty() {
        let mut best: Option<(usize, &Variant)> = None;

        for (key, value) in &file.variants {
            let Some(score) = match_score(key, state) else {
                continue;
            };
            let Some(variant) = value.first() else {
                continue;
            };
            if best.is_none_or(|(best_score, _)| score > best_score) {
                best = Some((score, variant));
            }
        }

        return best
            .map(|(_, variant)| vec![variant.clone()])
            .unwrap_or_default();
    }

    let mut selected = Vec::new();
    for part in &file.multipart {
        let applies = part
            .when
            .as_ref()
            .is_none_or(|condition| condition_matches(condition, state));
        if applies && let Some(variant) = part.apply.first() {
            selected.push(variant.clone());
        }
    }
    selected
}

fn match_score(key: &str, state: &BlockStateKey) -> Option<usize> {
    if key.is_empty() {
        return Some(0);
    }

    let mut score = 0usize;
    for part in key.split(',') {
        let (name, expected) = part.trim().split_once('=')?;
        let actual = state.properties.get(name)?;
        if !value_matches(actual, expected) {
            return None;
        }
        score += 1;
    }
    Some(score)
}

fn condition_matches(condition: &Condition, state: &BlockStateKey) -> bool {
    match condition {
        Condition::And(and) => and.and.iter().all(|item| condition_matches(item, state)),
        Condition::Or(or) => or.or.iter().any(|item| condition_matches(item, state)),
        Condition::Props(properties) => properties.iter().all(|(name, expected)| {
            state
                .properties
                .get(name)
                .is_some_and(|actual| value_matches(actual, expected))
        }),
    }
}

fn value_matches(actual: &str, expected: &str) -> bool {
    expected.split('|').any(|value| value == actual)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn make_state(name: &str, pairs: &[(&str, &str)]) -> BlockStateKey {
        BlockStateKey::new(
            name,
            pairs
                .iter()
                .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
                .collect::<BTreeMap<_, _>>(),
        )
    }

    fn file(json: &str) -> BlockstateFile {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn variant_key_matches_a_subset_and_prefers_specificity() {
        let file = file(
            r#"{
                "variants": {
                    "": { "model": "default" },
                    "facing=north": { "model": "north" },
                    "facing=north,half=top": { "model": "north_top" }
                }
            }"#,
        );

        let state = make_state(
            "minecraft:test",
            &[("facing", "north"), ("half", "top"), ("open", "false")],
        );
        let variants = select_variants(&file, &state);
        assert_eq!(variants[0].model, "north_top");

        let state = make_state("minecraft:test", &[("facing", "south")]);
        let variants = select_variants(&file, &state);
        assert_eq!(variants[0].model, "default");
    }

    #[test]
    fn multipart_handles_or_and_alternatives() {
        let file = file(
            r#"{
                "multipart": [
                    { "apply": { "model": "post" } },
                    {
                        "apply": { "model": "east" },
                        "when": { "OR": [ { "east": "side|up" }, { "west": "side" } ] }
                    }
                ]
            }"#,
        );

        let state = make_state("minecraft:test", &[("east", "up")]);
        let models: Vec<_> = select_variants(&file, &state)
            .into_iter()
            .map(|variant| variant.model)
            .collect();
        assert_eq!(models, vec!["post", "east"]);
    }

    #[test]
    fn multipart_handles_and() {
        let file = file(
            r#"{
                "multipart": [
                    {
                        "apply": { "model": "both" },
                        "when": { "AND": [ { "north": "true" }, { "south": "true" } ] }
                    }
                ]
            }"#,
        );

        let state = make_state("minecraft:test", &[("north", "true"), ("south", "true")]);
        assert_eq!(select_variants(&file, &state).len(), 1);

        let state = make_state("minecraft:test", &[("north", "true")]);
        assert!(select_variants(&file, &state).is_empty());
    }
}
