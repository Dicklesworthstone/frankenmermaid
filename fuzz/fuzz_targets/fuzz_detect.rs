#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(input) = std::str::from_utf8(data) {
        // detect_type must not panic on any input.
        let _diagram_type = fm_parser::detect_type(input);

        // detect_type_with_confidence must not panic and must produce bounded confidence.
        let detected = fm_parser::detect_type_with_confidence(input);
        assert!(
            (0.0..=1.0).contains(&detected.confidence),
            "confidence out of range: {}",
            detected.confidence
        );

        // Detection must be deterministic.
        let detected2 = fm_parser::detect_type_with_confidence(input);
        assert_eq!(detected.diagram_type, detected2.diagram_type);
        assert_eq!(detected.confidence, detected2.confidence);
    }
});
