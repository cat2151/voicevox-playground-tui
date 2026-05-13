use super::*;
use mascot_render_client::{preview_mouth_flap_timeline_request, PREVIEW_MOUTH_FLAP_FPS};
use mascot_render_protocol::{ChangeCharacterRequest, MotionTimelineKind};
use std::net::SocketAddr;

#[test]
fn motion_timeline_request_serializes_mouth_flap_kind() {
    let body = serde_json::to_value(preview_mouth_flap_timeline_request()).unwrap();

    assert_eq!(body["steps"][0]["kind"], "mouth_flap");
}

#[test]
fn motion_timeline_request_uses_preview_mouth_flap_timing() {
    let request = motion_timeline_request(1_234);
    let preview_request = preview_mouth_flap_timeline_request();

    assert!(!preview_request.steps.is_empty());
    assert_eq!(request.steps.len(), preview_request.steps.len());
    assert!(matches!(
        request.steps[0].kind,
        MotionTimelineKind::MouthFlap
    ));
    assert_ne!(
        request.steps[0].duration_ms,
        preview_request.steps[0].duration_ms
    );
    assert_eq!(request.steps[0].duration_ms, 1_234);
    assert_eq!(request.steps[0].fps, PREVIEW_MOUTH_FLAP_FPS);
    assert_eq!(request.steps[0].kind, preview_request.steps[0].kind);
    assert_eq!(request.steps[0].fps, preview_request.steps[0].fps);
    if request.steps.len() > 1 {
        assert_eq!(&request.steps[1..], &preview_request.steps[1..]);
    }
}

#[test]
fn motion_timeline_request_body_serializes_target_character_name() {
    let request = motion_timeline_request(1_234);
    let body =
        serde_json::to_value(motion_timeline_request_body(&request, Some("四国めたん"))).unwrap();

    assert_eq!(body["steps"][0]["kind"], "mouth_flap");
    assert_eq!(body["target_character_name"], "四国めたん");
}

#[test]
fn motion_timeline_request_body_omits_empty_target_character_name() {
    let request = motion_timeline_request(1_234);
    let body = serde_json::to_value(motion_timeline_request_body(&request, None)).unwrap();

    assert_eq!(body["steps"][0]["kind"], "mouth_flap");
    assert!(body.get("target_character_name").is_none());
}

#[test]
fn format_mascot_request_without_body_omits_json_sections() {
    let address = SocketAddr::from(([127, 0, 0, 1], 62152));

    let request = format_mascot_request("POST", "/show", address, None);

    assert!(request.contains("header:"));
    assert!(request.contains("  POST /show HTTP/1.1"));
    assert!(request.contains("  Host: 127.0.0.1:62152"));
    assert!(request.contains("  Connection: close"));
    assert!(request.contains("  Content-Length: 0"));
    assert!(!request.contains("Content-Type: application/json"));
    assert!(!request.contains("body:"));
}

#[test]
fn format_mascot_json_request_pretty_prints_headers_and_body() {
    let address = SocketAddr::from(([127, 0, 0, 1], 62152));
    let body = ChangeCharacterRequest {
        character_name: "四国めたん".to_string(),
    };

    let request = format_mascot_json_request("POST", "/change-character", address, &body);

    let compact_body = serde_json::to_vec(&body).unwrap();
    assert!(request.contains("header:"));
    assert!(request.contains("  POST /change-character HTTP/1.1"));
    assert!(request.contains("  Host: 127.0.0.1:62152"));
    assert!(request.contains(&format!("  Content-Length: {}", compact_body.len())));
    assert!(request.contains("  Content-Type: application/json"));
    assert!(request.contains("body:"));
    assert!(request.contains("  {"));
    assert!(request.contains(r#"    "character_name": "四国めたん""#));
    assert!(request.contains("  }"));
}

#[test]
fn format_mascot_request_uses_brackets_for_ipv6_host_header() {
    let address = SocketAddr::from(([0, 0, 0, 0, 0, 0, 0, 1], 62152));

    let request = format_mascot_request("POST", "/show", address, None);

    assert!(request.contains("  Host: [::1]:62152"));
}
