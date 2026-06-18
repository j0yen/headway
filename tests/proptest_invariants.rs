//! Property-based invariants for headway.
//! READ-ONLY: the edit-agent must not modify this file.

use headway::Status;
use proptest::prelude::*;

proptest! {
    #[test]
    fn status_serializes_roundtrips(raw in prop_oneof![
        Just("built"),
        Just("no-op-fresh"),
        Just("cloudbuild-unreachable"),
        Just("build-failed"),
    ]) {
        let s: Status = serde_json::from_str(&format!("\"{raw}\"")).unwrap();
        let back = serde_json::to_string(&s).unwrap();
        let trimmed = back.trim_matches('"');
        prop_assert_eq!(trimmed, raw);
    }

    #[test]
    fn status_variants_are_not_built(_x in 0u8..3u8) {
        // All non-Built variants do not equal Built
        let non_built = [
            Status::NoOpFresh,
            Status::CloudbuildUnreachable,
            Status::BuildFailed,
        ];
        for s in &non_built {
            prop_assert_ne!(*s, Status::Built);
        }
    }
}
