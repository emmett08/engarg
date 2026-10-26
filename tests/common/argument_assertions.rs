//! Compare authored arguments while verifying independently retained host history.
use engarg::{
    domain::{ArgumentCase, Request},
    engine::{CallRecord, reconcile_retained_argument},
};
use serde_json::Value;

pub fn assert_authored_argument(report: &Value, expected: &Value) {
    let request: Request = serde_json::from_value(report["request"].clone()).unwrap();
    let canonical: ArgumentCase = serde_json::from_value(report["argument"].clone()).unwrap();
    let calls: Vec<CallRecord> = serde_json::from_value(report["transcript"].clone()).unwrap();
    let mut authored: ArgumentCase = serde_json::from_value(expected.clone()).unwrap();
    reconcile_retained_argument(&request, &mut authored, &canonical, &calls).unwrap();
    assert_eq!(serde_json::to_value(authored).unwrap(), report["argument"]);
    let history = canonical
        .dissent_revisions
        .as_ref()
        .expect("new runs retain origins");
    assert_eq!(history.schema, engarg::domain::DISSENT_REVISIONS_SCHEMA);
    assert_eq!(history.origins.len(), canonical.defeaters.len());
}
