//! Compile the repository writing skill and shared glossary into trusted prompts.

use crate::glossary;
use std::fmt::Write;

const WRITING: &str = include_str!("../../skills/write-engineering-arguments/SKILL.md");

fn body(skill: &str) -> &str {
    skill
        .strip_prefix("---\n")
        .and_then(|text| text.split_once("\n---\n"))
        .map_or(skill, |(_, body)| body.trim())
}

pub(super) fn append(prompt: &mut String) {
    prompt.push_str("\n\nENGINEERING WRITING SKILL\n");
    prompt.push_str(body(WRITING));
    prompt.push_str("\n\nARGUMENT ROLE GLOSSARY\nUse these definitions to keep each part's function distinct during construction, rewriting and critique. Codes label roles, not compulsory identifier prefixes. Determine a record's role from its declared type and references. Definitions supply vocabulary, not evidence. Keep definitions in reader help rather than repeating them in argument prose.\n");
    // Examples and provenance remain available in offline help. Generation needs
    // only the role, its meaning and its canonical representation.
    for entry in glossary::entries() {
        writeln!(
            prompt,
            "{} — {}. {} Field: {}",
            entry.code, entry.name, entry.definition, entry.field
        )
        .unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trusted_instructions_use_skill_body_and_canonical_definitions() {
        let mut prompt = String::new();
        append(&mut prompt);
        assert!(prompt.contains(body(WRITING)));
        assert!(!prompt.contains("name: write-engineering-arguments"));
        for entry in glossary::entries() {
            assert!(prompt.contains(&entry.definition), "missing {}", entry.code);
            assert!(
                prompt.contains(&entry.field),
                "missing {} field",
                entry.code
            );
        }
    }
}
