use proptest::prelude::*;
use voltlane_core::{
    fixtures::demo_project,
    persistence::{load_project, save_project},
};

fn no_panic_load(path: &std::path::Path) -> bool {
    std::panic::catch_unwind(|| {
        let _ = load_project(path);
    })
    .is_ok()
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 64,
        .. ProptestConfig::default()
    })]

    #[test]
    fn random_project_bytes_do_not_panic(raw in prop::collection::vec(any::<u8>(), 0..4096)) {
        let temp = tempfile::tempdir().expect("tempdir should be creatable");
        let path = temp.path().join("corrupt_random.voltlane.json");
        std::fs::write(&path, raw).expect("writing random payload should work");
        prop_assert!(no_panic_load(&path));
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 64,
        .. ProptestConfig::default()
    })]

    #[test]
    fn truncated_project_payloads_do_not_panic(prefix_len in 0usize..8192usize) {
        let temp = tempfile::tempdir().expect("tempdir should be creatable");
        let path = temp.path().join("corrupt_truncated.voltlane.json");
        save_project(&path, &demo_project()).expect("saving fixture project should work");

        let mut payload = std::fs::read(&path).expect("reading saved project should work");
        let truncated_len = prefix_len.min(payload.len());
        payload.truncate(truncated_len);
        std::fs::write(&path, payload).expect("writing truncated payload should work");

        prop_assert!(no_panic_load(&path));
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 64,
        .. ProptestConfig::default()
    })]

    #[test]
    fn mutated_project_payloads_do_not_panic(index in 0usize..8192usize, delta in any::<u8>()) {
        let temp = tempfile::tempdir().expect("tempdir should be creatable");
        let path = temp.path().join("corrupt_mutated.voltlane.json");
        save_project(&path, &demo_project()).expect("saving fixture project should work");

        let mut payload = std::fs::read(&path).expect("reading saved project should work");
        if !payload.is_empty() {
            let target = index % payload.len();
            payload[target] ^= delta.max(1);
        }
        std::fs::write(&path, payload).expect("writing mutated payload should work");

        prop_assert!(no_panic_load(&path));
    }
}
