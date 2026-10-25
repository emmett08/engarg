//! Shared argument-role definitions for prompts, offline lookup and reader help.
//!
//! Codes describe roles. Canonical record types and references establish identity,
//! independently of the spelling or prefix of a user-supplied identifier.

use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

/// The same definitions ship with the glossary skill and the compiled program.
pub const SOURCE: &str =
    include_str!("../skills/explain-engineering-arguments/references/roles.json");

/// One role or concept in EngArg's extended argument model.
#[derive(Debug, Deserialize, Serialize)]
pub struct GlossaryEntry {
    pub code: String,
    pub name: String,
    pub definition: String,
    pub question: String,
    pub example: String,
    pub origin: String,
    pub field: String,
}

static ENTRIES: LazyLock<Vec<GlossaryEntry>> = LazyLock::new(|| {
    serde_json::from_str(SOURCE).expect("the bundled argument glossary must be valid JSON")
});

/// Return every definition in the authored order.
#[must_use]
pub fn entries() -> &'static [GlossaryEntry] {
    &ENTRIES
}

/// Look up one conceptual code, ignoring case and surrounding whitespace.
/// Record identifiers such as `N42` require their canonical record context.
#[must_use]
pub fn find(code: &str) -> Option<&'static GlossaryEntry> {
    entries()
        .iter()
        .find(|entry| entry.code.eq_ignore_ascii_case(code.trim()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn every_definition_is_complete_and_has_a_unique_code() {
        let mut codes = HashSet::new();
        for entry in entries() {
            assert!(codes.insert(entry.code.to_ascii_lowercase()));
            for text in [
                &entry.code,
                &entry.name,
                &entry.definition,
                &entry.question,
                &entry.example,
                &entry.origin,
                &entry.field,
            ] {
                assert!(!text.trim().is_empty(), "incomplete {} entry", entry.code);
            }
        }
        for code in ["C", "S", "G", "E", "W", "B", "Q", "R", "A", "D", "N", "ARG"] {
            assert!(find(code).is_some(), "missing role {code}");
        }
    }

    #[test]
    fn lookup_accepts_codes_without_guessing_record_types() {
        assert_eq!(find(" w ").unwrap().code, "W");
        assert!(find("W42").is_none());
        assert!(find("constructor-1").is_none());
        assert!(find("unrecognised").is_none());
    }
}
