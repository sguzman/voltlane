use std::{env, path::Path};

use voltlane_core::{
    fixtures::demo_project,
    generate_parity_report,
    parity::{read_parity_report, write_parity_report},
};

#[test]
fn parity_report_matches_golden_baseline() {
    let baseline_path = Path::new("tests/fixtures/parity_baseline.json");
    let report = generate_parity_report(&demo_project()).expect("parity generation should work");

    if env::var("UPDATE_PARITY_BASELINE").as_deref() == Ok("1") {
        write_parity_report(baseline_path, &report).expect("baseline update should succeed");
    }

    let baseline = read_parity_report(baseline_path).expect("baseline must exist and parse");
    assert_eq!(
        report, baseline,
        "parity report drifted from golden baseline"
    );
}
