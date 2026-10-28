use engarg::{
    domain::{ArgumentCase, Request},
    glossary,
    presentations::{self, RenderError, RenderLimits},
};
use std::process::Command;

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_engarg"))
}

#[test]
fn offline_lookup_and_json_share_the_canonical_role_definition() {
    let output = cli()
        .env_remove("ENGARG_API_KEY")
        .env_remove("ENGARG_MODEL")
        .args(["glossary", "w"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    let warrant = glossary::find("W").unwrap();
    assert!(text.contains(&warrant.definition));
    assert!(text.contains(&warrant.question));
    assert!(text.contains(&warrant.example));

    let output = cli().args(["glossary", "W", "--json"]).output().unwrap();
    assert!(output.status.success());
    let actual: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(actual, serde_json::to_value(warrant).unwrap());

    let output = cli().args(["glossary", "--json"]).output().unwrap();
    assert!(output.status.success());
    let actual: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(actual, serde_json::to_value(glossary::entries()).unwrap());
}

#[test]
fn an_unknown_role_code_reports_an_actionable_error() {
    let output = cli().args(["glossary", "N42"]).output().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("engarg glossary"));
    assert!(output.stdout.is_empty());
}

#[test]
fn html_keeps_canonical_definitions_once_and_links_tooltips_to_the_closed_glossary() {
    let request: Request = serde_json::from_str(include_str!("../examples/request.json")).unwrap();
    let case: ArgumentCase = serde_json::from_str(include_str!("../examples/case.json")).unwrap();
    let output = presentations::html(&request, &case).unwrap();
    assert!(output.contains("href=\"#glossary\""));
    let glossary_body = output
        .split_once("<details id=\"glossary\">")
        .unwrap()
        .1
        .split_once("</details>")
        .unwrap()
        .0;
    let (summary, body) = glossary_body.split_once("</summary>").unwrap();
    for entry in glossary::entries() {
        assert!(!summary.contains(&entry.definition));
        // Definitions contain plain prose today. Check through escaped output so
        // future mathematical or quoted notation remains text rather than HTML.
        let definition = entry
            .definition
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&#39;");
        assert!(body.contains(&definition), "missing {}", entry.code);
        assert_eq!(body.matches(&definition).count(), 1, "{}", entry.code);
        let target = format!("glossary-{}", anchor(&entry.code));
        assert_eq!(output.matches(&format!("id=\"{target}\"")).count(), 1);
    }

    let limits = RenderLimits {
        max_output_bytes: output.len() - 1,
        ..Default::default()
    };
    assert_eq!(
        presentations::html_with_limits(&request, &case, None, limits),
        Err(RenderError::ResourceLimit("output bytes"))
    );
}

fn anchor(id: &str) -> String {
    format!(
        "node-{}",
        id.as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}
