pub mod encoding;
pub mod engine;
pub mod markers;
pub mod matching;
pub mod payloads;
pub mod types;

pub use encoding::*;
pub use engine::*;
pub use markers::*;
pub use matching::*;
pub use payloads::*;
pub use types::*;

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn payload_config(values: &[&str]) -> InvokerPayloadConfig {
        InvokerPayloadConfig {
            payload_type: InvokerPayloadType::SimpleList,
            values: values.iter().map(|value| value.to_string()).collect(),
            file_path: None,
            number_start: None,
            number_end: None,
            number_step: None,
            number_format: None,
            processing: Vec::new(),
        }
    }

    fn attack_config(
        position_payloads: Option<HashMap<String, InvokerPayloadConfig>>,
    ) -> InvokerAttackConfig {
        InvokerAttackConfig {
            name: "test".to_string(),
            mode: InvokerAttackMode::Sniper,
            base_request: InvokerHttpRequest {
                method: "GET".to_string(),
                url: "https://example.test/login?u=§user§&p=§pass§".to_string(),
                headers: HashMap::new(),
                body: String::new(),
                follow_redirects: true,
                max_hops: 10,
            },
            positions: vec![
                InvokerPayloadPosition {
                    name: "position_1".to_string(),
                    start: 0,
                    end: 0,
                },
                InvokerPayloadPosition {
                    name: "position_2".to_string(),
                    start: 0,
                    end: 0,
                },
            ],
            payload_config: payload_config(&["legacy-a", "legacy-b"]),
            position_payloads,
            concurrency: 1,
            delay_ms: 0,
            delay_max_ms: None,
            retries: 0,
            grep_match: InvokerGrepMatchConfig {
                enabled: false,
                keyword: String::new(),
                case_sensitive: false,
            },
            grep_extract: InvokerGrepExtractConfig {
                enabled: false,
                regex: String::new(),
                replacement: None,
            },
            session_handling: InvokerSessionHandlingConfig {
                enabled: false,
                extract_token_name: None,
                extract_from_response: None,
                update_header_name: None,
            },
        }
    }

    #[test]
    fn aligned_position_payloads_pair_equal_length_sets() {
        let sources = vec![
            (
                "position_1".to_string(),
                vec!["admin".to_string(), "root".to_string()],
            ),
            (
                "position_2".to_string(),
                vec!["pass".to_string(), "toor".to_string()],
            ),
        ];

        let rows = generate_aligned_position_payloads(&sources);

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].get("position_1").map(String::as_str), Some("admin"));
        assert_eq!(rows[0].get("position_2").map(String::as_str), Some("pass"));
        assert_eq!(rows[1].get("position_1").map(String::as_str), Some("root"));
        assert_eq!(rows[1].get("position_2").map(String::as_str), Some("toor"));
    }

    #[test]
    fn aligned_position_payloads_stop_at_shortest_set() {
        let sources = vec![
            (
                "position_1".to_string(),
                vec!["admin".to_string(), "root".to_string(), "test".to_string()],
            ),
            ("position_2".to_string(), vec!["pass".to_string()]),
        ];

        let rows = generate_aligned_position_payloads(&sources);

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].get("position_1").map(String::as_str), Some("admin"));
        assert_eq!(rows[0].get("position_2").map(String::as_str), Some("pass"));
    }

    #[test]
    fn validation_requires_payloads_for_every_marker() {
        let mut position_payloads = HashMap::new();
        position_payloads.insert("position_1".to_string(), payload_config(&["admin"]));

        let config = attack_config(Some(position_payloads));

        assert_eq!(
            validate_invoker_config(&config),
            Err("Add payloads for position_2".to_string())
        );
    }

    #[test]
    fn legacy_single_payload_config_still_generates_sniper_rows() {
        let config = attack_config(None);
        let rows = build_invoker_payload_rows(&config, 2).expect("legacy rows");

        assert_eq!(rows.len(), 4);
        assert_eq!(
            rows[0].get("position_1").map(String::as_str),
            Some("legacy-a")
        );
        assert!(!rows[0].contains_key("position_2"));
    }

    #[test]
    fn sequential_marker_tracking_across_url_headers_and_body() {
        let mut headers = HashMap::new();
        headers.insert("Authorization".to_string(), "Bearer §token123§".to_string());

        let req = InvokerHttpRequest {
            method: "POST".to_string(),
            url: "https://example.com/api?id=§id123§".to_string(),
            headers,
            body: "{\"user\":\"§alice§\"}".to_string(),
            follow_redirects: true,
            max_hops: 10,
        };

        let defaults = marker_defaults(&req);
        assert_eq!(defaults, vec!["id123", "token123", "alice"]);

        let mut payload_values = HashMap::new();
        payload_values.insert("position_1".to_string(), "999".to_string());
        payload_values.insert("position_2".to_string(), "secret_jwt".to_string());
        payload_values.insert("position_3".to_string(), "bob".to_string());

        let mut position_index = 0;
        let url = replace_marked_values_tracked(
            &req.url,
            &payload_values,
            &defaults,
            &mut position_index,
        );
        assert_eq!(url, "https://example.com/api?id=999");
        assert_eq!(position_index, 1);

        let mut sorted_headers: Vec<_> = req.headers.iter().collect();
        sorted_headers.sort_by_key(|(k, _)| k.to_lowercase());
        for (_name, value) in sorted_headers {
            let replaced_h = replace_marked_values_tracked(
                value,
                &payload_values,
                &defaults,
                &mut position_index,
            );
            assert_eq!(replaced_h, "Bearer secret_jwt");
        }
        assert_eq!(position_index, 2);

        let body = replace_marked_values_tracked(
            &req.body,
            &payload_values,
            &defaults,
            &mut position_index,
        );
        assert_eq!(body, "{\"user\":\"bob\"}");
        assert_eq!(position_index, 3);
    }
}
