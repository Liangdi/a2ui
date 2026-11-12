//! Timing corpus is opt-in and serial, as required by the official harness.
use a2ui_base::{message_processor::MessageProcessor, validate::ValidationConfig};
use serde_json::Value;
use std::time::{Duration, Instant};

fn expand(parts: &[Value], size: usize) -> String {
    parts
        .iter()
        .map(|part| {
            if let Some(text) = part.as_str() {
                return text.to_owned();
            }
            let template = part["repeat"].as_str().expect("repeat template");
            let separator = part["separator"].as_str().unwrap_or("");
            (0..size)
                .map(|i| template.replace("{i}", &i.to_string()))
                .collect::<Vec<_>>()
                .join(separator)
        })
        .collect()
}

fn catalog(path: &str) -> Value {
    let json = match path {
        "catalogs/basic/v1/catalog.json" => {
            include_str!("conformance/catalog-definitions/basic.json")
        }
        "conformance/test_data/catalogs/composition_catalog_v1_0.json" => {
            include_str!("conformance/catalog-definitions/composition_catalog_v1_0.json")
        }
        _ => panic!("unhandled catalog: {path}"),
    };
    serde_json::from_str(json).unwrap()
}

fn measure(case: &Value, size: usize) -> (Duration, Duration) {
    // Payload expansion/decoding and catalog JSON decoding are outside timing.
    let payload = expand(case["payload"].as_array().unwrap(), size);
    let messages: Vec<Value> = serde_json::from_str(&payload).unwrap();
    let messages = messages
        .iter()
        .map(|v| MessageProcessor::parse_message(&v.to_string()).unwrap())
        .collect();
    let catalogs: Vec<_> = case["catalogPaths"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| catalog(p.as_str().unwrap()))
        .collect();
    let start = Instant::now();
    let mut processor = MessageProcessor::new(vec![]);
    for catalog in catalogs {
        processor.register_inline_catalog(catalog).unwrap();
    }
    match case["args"]["validation_config"].as_str().unwrap() {
        "none" => {}
        "relaxed" => processor = processor.with_validation(ValidationConfig::RELAXED),
        other => panic!("unhandled validation mode: {other}"),
    }
    let processing_start = Instant::now();
    let results = processor.process_messages(messages);
    let processing = processing_start.elapsed();
    let elapsed = start.elapsed();
    for result in results {
        result.unwrap();
    }
    let surface = processor.model.get_surface("s1").unwrap();
    assert_eq!(surface.components.borrow().all().len(), size + 1);
    let components = surface.components.borrow();
    assert_eq!(
        components
            .get(&format!("item_{}", size - 1))
            .unwrap()
            .properties["text"],
        format!("Item number {}", size - 1)
    );
    (elapsed, processing)
}

#[test]
#[ignore = "machine-sensitive official scaling corpus; run alone with --ignored --test-threads=1 --nocapture"]
fn official_message_processor_growth_measurements() {
    let cases: Vec<Value> =
        serde_yaml_ng::from_str(include_str!("conformance/message_processor_scaling.yaml"))
            .unwrap();
    assert_eq!(cases.len(), 3);
    for case in cases {
        assert_eq!(case["action"], "measure_growth");
        assert_eq!(case["operation"], "process_messages");
        let sizes: Vec<_> = case["sizes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as usize)
            .collect();
        assert_eq!(sizes.len(), 2);
        for size in &sizes {
            measure(&case, *size);
        }
        let mut samples = [vec![], vec![]];
        for _ in 0..5 {
            for (i, size) in sizes.iter().enumerate() {
                samples[i].push(measure(&case, *size));
            }
        }
        let median = |index: usize, processing: bool| {
            let mut values: Vec<_> = samples[index]
                .iter()
                .map(|(total, work)| if processing { *work } else { *total })
                .collect();
            values.sort();
            values[2]
        };
        let medians = [median(0, false), median(1, false)];
        let work_medians = [median(0, true), median(1, true)];
        let ratio = medians[1].as_secs_f64() / medians[0].as_secs_f64();
        let work_ratio = work_medians[1].as_secs_f64() / work_medians[0].as_secs_f64();
        eprintln!(
            "processing-only medians {:?}/{:?}, ratio={work_ratio:.3}",
            work_medians[1], work_medians[0]
        );
        let composition = case["name"].as_str().unwrap().contains("composition");
        eprintln!(
            "{}: {:?}/{:?} ratio={ratio:.3}; {}",
            case["name"],
            medians[1],
            medians[0],
            if composition {
                "MEASURED ONLY: composition constraints unsupported"
            } else {
                "processor timing (not full schema-validation conformance)"
            }
        );
        // A favorable ratio cannot establish checks the implementation never runs.
        // Composition remains an explicit semantic gap, but is actually measured.
        assert!(
            work_ratio <= case["max_ratio"].as_f64().unwrap(),
            "processing-only growth exceeds bound: {work_ratio}"
        );
        assert!(
            ratio <= case["max_ratio"].as_f64().unwrap(),
            "{} exceeded official growth bound: {ratio}",
            case["name"]
        );
    }
}
