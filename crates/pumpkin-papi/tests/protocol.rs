//! Tests for the parts that are easy to break quietly: the token scanner, the
//! JSON boundary, and the expansion table. None of it needs a server.

use pumpkin_papi::protocol::{
    Cache, MAX_CACHE_TTL_MS, ProtocolError, Request, Response, Success, check_name,
    check_namespace, decode_request, decode_response, encode,
};
use pumpkin_papi::tokens::{find, substitute};
use pumpkin_papi::Registry;

/// A resolver over a fixed table, so a test can say what exists and what does not.
fn resolve_all<'a>(
    pairs: &'a [(&'a str, &'a str)],
) -> impl Fn(&str, Option<&str>) -> Option<String> + 'a {
    move |id, _argument| {
        pairs
            .iter()
            .find(|(key, _)| *key == id)
            .map(|(_, value)| (*value).to_string())
    }
}

mod tokens {
    use super::*;

    #[test]
    fn replaces_a_single_token() {
        let out = substitute("hello %player_name%", resolve_all(&[("player_name", "Steve")]));
        assert_eq!(out.text, "hello Steve");
        assert!(out.unresolved.is_empty());
    }

    #[test]
    fn replaces_every_occurrence() {
        let out = substitute(
            "%player_ping% ms and %player_ping% ms again",
            resolve_all(&[("player_ping", "42")]),
        );
        assert_eq!(out.text, "42 ms and 42 ms again");
    }

    #[test]
    fn leaves_an_unresolved_token_exactly_as_written() {
        let out = substitute("%known% %missing% %missing%", resolve_all(&[("known", "yes")]));
        assert_eq!(out.text, "yes %missing% %missing%");
        assert_eq!(out.unresolved, vec!["missing".to_string()]);
    }

    #[test]
    fn a_repeated_token_is_only_resolved_once() {
        let mut asked = 0;
        let out = substitute(
            "%a% %a% %a%",
            |_, _| {
                asked += 1;
                Some("x".to_string())
            },
        );
        assert_eq!(out.text, "x x x");
        assert_eq!(asked, 1, "a repeated token should cost one lookup");
    }

    #[test]
    fn an_empty_value_still_counts_as_resolved() {
        let out = substitute("[%player_name%]", resolve_all(&[("player_name", "")]));
        assert_eq!(out.text, "[]");
        assert!(out.unresolved.is_empty());
    }

    #[test]
    fn there_is_no_escape_for_a_percent_sign() {
        let out = substitute("%%player_name%%", resolve_all(&[("player_name", "Steve")]));
        assert_eq!(out.text, "%Steve%");
    }

    #[test]
    fn a_lone_percent_sign_is_left_alone() {
        for source in ["50% off", "%", "% ", "%player_name", "a % b %", "100%%"] {
            let out = substitute(source, resolve_all(&[("player_name", "Steve")]));
            assert_eq!(out.text, source, "source {source:?} should be untouched");
            assert!(out.unresolved.is_empty());
        }
    }

    #[test]
    fn an_argument_reaches_the_resolver() {
        let mut seen: Vec<Option<String>> = Vec::new();
        let out = substitute(
            "%player_has_permission:some.node%",
            |_id, argument| {
                seen.push(argument.map(str::to_string));
                Some("true".to_string())
            },
        );
        assert_eq!(out.text, "true");
        assert_eq!(seen, vec![Some("some.node".to_string())]);
    }

    #[test]
    fn find_reports_each_token_once_in_order() {
        let found = find("%a% %b:x% %a%");
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].id, "a");
        assert_eq!(found[1].id, "b");
        assert_eq!(found[1].argument.as_deref(), Some("x"));
    }

    #[test]
    fn multibyte_text_survives_a_scan() {
        let out = substitute("héllo %player_name% wörld ☃", resolve_all(&[("player_name", "Steve")]));
        assert_eq!(out.text, "héllo Steve wörld ☃");
    }
}

mod wire {
    use super::*;

    #[test]
    fn a_request_round_trips() {
        let request = Request::SetPlaceholders {
            text: "%player_name%".to_string(),
            viewer: Some("Steve".to_string()),
            argument: None,
        };
        let bytes = encode(&request).unwrap();
        assert_eq!(decode_request(&bytes).unwrap(), request);
    }

    #[test]
    fn the_op_tag_is_snake_case() {
        let bytes = encode(&Request::Ping).unwrap();
        assert_eq!(std::str::from_utf8(&bytes).unwrap(), r#"{"op":"ping"}"#);
    }

    #[test]
    fn an_unknown_op_is_rejected() {
        let bytes = br#"{"op":"explode"}"#;
        assert!(decode_request(bytes).is_err());
    }

    #[test]
    fn malformed_messages_are_rejected() {
        for bytes in [b"{oops".as_slice(), b"[]", b"", b"{\"op\":"] {
            assert!(decode_request(bytes).is_err(), "should reject {bytes:?}");
        }
    }

    #[test]
    fn invalid_utf8_is_rejected() {
        assert!(decode_request(b"{\"op\":\"\xff\"}").is_err());
    }

    #[test]
    fn a_response_round_trips_and_keeps_its_flattened_body() {
        let response = Response::success(Success::ResolvedBatch {
            results: vec![pumpkin_papi::protocol::ResolvedLine {
                text: "42ms".to_string(),
                unresolved: vec![],
            }],
        });
        let bytes = encode(&response).unwrap();
        let json = std::str::from_utf8(&bytes).unwrap();
        assert!(json.contains(r#""ok":true"#), "got {json}");
        assert!(json.contains(r#""results""#), "got {json}");
        assert_eq!(decode_response(&bytes).unwrap(), response);
    }

    #[test]
    fn a_failed_response_carries_no_body() {
        let response = Response::from_error(ProtocolError::Reserved {
            namespace: "player".to_string(),
        });
        assert!(!response.ok);
        let message = response.error.clone().expect("a failure carries a reason");
        assert!(message.contains("reserved"));
        assert!(response.into_success().is_err());
    }

    #[test]
    fn a_response_without_ok_is_rejected() {
        assert!(decode_response(br#"{"text":"x"}"#).is_err());
    }

    #[test]
    fn an_unknown_field_is_ignored_rather_than_fatal() {
        let bytes = br#"{"op":"set_placeholders","text":"x","future":true}"#;
        assert!(decode_request(bytes).is_ok());
    }
}

mod namespaces {
    use super::*;

    #[test]
    fn a_normal_namespace_is_accepted() {
        assert!(check_namespace("luckperms").is_ok());
    }

    #[test]
    fn reserved_namespaces_are_refused() {
        for namespace in ["player", "server", "papi"] {
            assert!(
                matches!(
                    check_namespace(namespace),
                    Err(ProtocolError::Reserved { .. })
                ),
                "{namespace} should be reserved"
            );
        }
    }

    #[test]
    fn a_malformed_namespace_is_refused() {
        for namespace in ["", "2ranks", "my-ranks", "My Ranks"] {
            assert!(check_namespace(namespace).is_err(), "{namespace:?} should fail");
        }
    }

    #[test]
    fn a_malformed_name_is_refused() {
        for name in ["", "pre-fix", "pre fix"] {
            assert!(check_name(name).is_err(), "{name:?} should fail");
        }
    }

    #[test]
    fn a_ttl_is_clamped_to_the_maximum() {
        assert_eq!(Cache::Never.effective_ttl_ms(), 0);
        assert_eq!(Cache::Ttl { ms: 5_000 }.effective_ttl_ms(), 5_000);
        assert_eq!(
            Cache::Ttl { ms: u64::MAX }.effective_ttl_ms(),
            MAX_CACHE_TTL_MS
        );
    }
}

mod expansions {
    use super::*;

    #[test]
    fn a_namespace_is_claimed_by_its_first_segment() {
        let mut registry = Registry::new();
        registry
            .add("ranks", "Ranks", ["prefix".to_string()], Cache::Never)
            .unwrap();
        assert!(registry.owner_of("ranks_prefix").is_some());
        assert!(registry.owner_of("RANKS_prefix").is_some());
        assert!(registry.owner_of("ranks").is_none());
        assert!(registry.owner_of("other_prefix").is_none());
    }

    #[test]
    fn registering_again_replaces_the_names() {
        let mut registry = Registry::new();
        registry
            .add("ranks", "Ranks", ["prefix".to_string(), "suffix".to_string()], Cache::Never)
            .unwrap();
        let expansion = registry
            .add("ranks", "Ranks", ["prefix".to_string()], Cache::Ttl { ms: 1_000 })
            .unwrap()
            .clone();
        assert_eq!(registry.len(), 1);
        assert_eq!(expansion.names.len(), 1);
        assert_eq!(expansion.ttl_ms(), 1_000);
    }

    #[test]
    fn an_empty_name_list_answers_anything() {
        let mut registry = Registry::new();
        let expansion = registry.add("ranks", "Ranks", [], Cache::Never).unwrap();
        assert!(expansion.names.is_empty());
        assert!(expansion.declares("anything_at_all"));
    }

    #[test]
    fn a_declared_name_list_is_honoured() {
        let mut registry = Registry::new();
        let expansion = registry
            .add("ranks", "Ranks", ["prefix".to_string()], Cache::Never)
            .unwrap();
        assert!(expansion.declares("prefix"));
        assert!(!expansion.declares("something_else"));
    }

    #[test]
    fn dropping_a_source_leaves_other_contributors_alone() {
        let mut registry = Registry::new();
        registry.add("ranks", "Ranks", ["prefix".to_string()], Cache::Never).unwrap();
        registry.add("quests", "Ranks", ["stage".to_string()], Cache::Never).unwrap();
        registry.add("zones", "Zones", ["name".to_string()], Cache::Never).unwrap();

        assert_eq!(registry.drop_source("Ranks"), vec!["quests".to_string(), "ranks".to_string()]);
        assert_eq!(registry.namespaces(), vec!["zones"]);
    }

    #[test]
    fn a_reserved_namespace_never_reaches_the_table() {
        let mut registry = Registry::new();
        assert!(registry.add("player", "Ranks", [], Cache::Never).is_err());
        assert!(registry.is_empty());
    }
}
