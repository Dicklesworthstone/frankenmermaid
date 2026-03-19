#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Only fuzz valid UTF-8 strings — the parser expects &str.
    if let Ok(input) = std::str::from_utf8(data) {
        // Must not panic on any input.
        let result = fm_parser::parse(input);

        // Confidence must be in [0.0, 1.0].
        assert!(
            (0.0..=1.0).contains(&result.confidence),
            "confidence out of range: {}",
            result.confidence
        );

        // IR must be serializable.
        let _ = serde_json::to_string(&result.ir);
    }
});
