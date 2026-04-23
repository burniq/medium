use overlay_protocol::{ServiceKind, SessionOpenRequest};

#[test]
fn session_open_request_round_trips_as_json() {
    let req = SessionOpenRequest {
        service_id: "svc_home_openclaw".into(),
        requester_device_id: "dev_phone".into(),
    };

    let json = serde_json::to_string(&req).unwrap();
    let parsed: SessionOpenRequest = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.service_id, "svc_home_openclaw");
    assert_eq!(parsed.requester_device_id, "dev_phone");
    assert_eq!(ServiceKind::Https.as_str(), "https");
}
