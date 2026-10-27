//! Direct EAL fixtures exercise admitted descriptors without the retired exporter.
use serde_json::Value;

pub fn source(version: &str, input: &Value, acquisition: &str) -> String {
    format!(
        "language \"EAL/4\"\nenvironment fixture {{\n  require true == true\n}}\ntool source {{\n  version {}\n}}\nevidence observation {{\n  tool source\n  kind observation\n  environment fixture\n  max_age 3600\n  input {}\n  require {}\n}}\nreasoning measured {{\n  method \"structured/1\"\n  rationale \"The declared acquisition condition was observed.\"\n}}\nclaim completed {{\n  statement \"The requested acquisition completed; measurement polarity remains in the observation.\"\n  environment fixture\n}}\nargument measured_acquisition = [evidence observation] via measured => completed\n",
        serde_json::to_string(version).unwrap(),
        input,
        acquisition,
    )
}
