//! What the plan says about units you cannot start with.

use std::path::Path;

use rust_anything::analyze::analyze;
use rust_anything::report;

#[test]
fn a_parse_error_names_a_status_that_exists_and_an_empty_start_here_explains_itself() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/broken-source");
    let report = analyze(&root).unwrap();
    let md = report::markdown(&report, "broken-source");
    // The statuses are ready, caution, blocked and keep: there is no "partial".
    assert!(
        md.contains("marked caution"),
        "the parse-error note should name a real status:\n{md}"
    );
    assert!(!md.contains("marked partial"), "{md}");
    // Nothing is blocked here, so pointing the reader at "the blocked units" sends them nowhere:
    // the reason nothing starts is that no unit exports a function a bridge could carry.
    assert!(
        md.contains("No unit exports anything a bridge can carry"),
        "an empty start_here should say why:\n{md}"
    );
    assert!(!md.contains("See the blocked units"), "{md}");
}
