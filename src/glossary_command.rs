//! Print the shared argument glossary without provider configuration or calls.

use engarg::{Error, glossary};

pub(crate) fn run(code: Option<&str>, json: bool) -> Result<(), Error> {
    let entries = glossary::entries();
    let selected = code
        .map(|code| {
            glossary::find(code).ok_or_else(|| {
                Error::InvalidInput(format!(
                    "unknown glossary code {code:?}; use engarg glossary to list role codes and concept keys"
                ))
            })
        })
        .transpose()?;
    if json {
        let value = match selected {
            Some(entry) => serde_json::to_value(entry)?,
            None => serde_json::to_value(entries)?,
        };
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else {
        println!(
            "Argument codes describe roles. Read a record's type and references to establish its role.\n"
        );
        for entry in selected.map_or(entries, std::slice::from_ref) {
            println!(
                "{} — {}\n{}\nQuestion: {}\nExample: {}\nModel origin: {}\nRepresentation: {}\n",
                entry.code,
                entry.name,
                entry.definition,
                entry.question,
                entry.example,
                entry.origin,
                entry.field
            );
        }
    }
    Ok(())
}
