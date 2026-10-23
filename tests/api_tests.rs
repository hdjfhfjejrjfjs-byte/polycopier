use polycopier::api::SetupPayload;

#[test]
fn test_setup_payload_deserialization() {
    // 1. Valid payload matching exactly what the new SetupWizard.tsx sends.
    // Notice `target_wallets` is intentionally omitted compared to the old payload.
    let raw_payload = r#"{
        "funder_address": "0x01234FUNDER",
        "private_key": "0x9999PRIVATE"
    }"#;

    let payload_result = serde_json::from_str::<SetupPayload>(raw_payload);
    assert!(
        payload_result.is_ok(),
        "SetupPayload should seamlessly deserialize without target_wallets"
    );

    let payload = payload_result.unwrap();
    assert_eq!(payload.funder_address, "0x01234FUNDER");
    assert_eq!(payload.private_key, "0x9999PRIVATE");
}

#[test]
fn test_setup_payload_rejects_missing_funder() {
    // 2. Erroneous payload missing the now-primary funder address
    let bad_payload = r#"{
        "private_key": "0x9999PRIVATE"
    }"#;

    let result = serde_json::from_str::<SetupPayload>(bad_payload);
    assert!(
        result.is_err(),
        "SetupPayload MUST reject requests missing the funder_address"
    );
}

#[test]
fn test_setup_payload_tolerates_unexpected_legacy_fields() {
    // 3. Test that if a Ghost Cache client somehow sends `target_wallets`, it is safety ignored
    // (Serde ignores unknown fields by default struct logic)
    let legacy_payload = r#"{
        "funder_address": "0x01234FUNDER",
        "private_key": "0x9999PRIVATE",
        "target_wallets": "0xAAAA,0xBBBB"
    }"#;

    let result = serde_json::from_str::<SetupPayload>(legacy_payload);
    assert!(
        result.is_ok(),
        "SetupPayload should gracefully ignore legacy phantom targets if provided"
    );
}

// ── Request guard (Host / Origin / token) ─────────────────────────────────

use axum::http::{HeaderMap, HeaderValue, StatusCode};
use polycopier::api::{check_request, env_status, TOKEN_HEADER};

const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn headers(host: &str, origin: Option<&str>, token: Option<&str>) -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert("host", HeaderValue::from_str(host).unwrap());
    if let Some(o) = origin {
        h.insert("origin", HeaderValue::from_str(o).unwrap());
    }
    if let Some(t) = token {
        h.insert(TOKEN_HEADER, HeaderValue::from_str(t).unwrap());
    }
    h
}

#[test]
fn guard_accepts_same_origin_with_token() {
    assert!(check_request(&headers("localhost:3000", None, Some(TOKEN)), TOKEN).is_ok());
    assert!(check_request(
        &headers("127.0.0.1:3000", Some("http://127.0.0.1:3000"), Some(TOKEN)),
        TOKEN
    )
    .is_ok());
}

#[test]
fn guard_rejects_missing_or_wrong_token() {
    assert_eq!(
        check_request(&headers("localhost:3000", None, None), TOKEN),
        Err(StatusCode::UNAUTHORIZED)
    );
    assert_eq!(
        check_request(&headers("localhost:3000", None, Some("deadbeef")), TOKEN),
        Err(StatusCode::UNAUTHORIZED)
    );
}

#[test]
fn guard_rejects_foreign_origin_and_rebinding_host() {
    assert_eq!(
        check_request(
            &headers("localhost:3000", Some("http://evil.com"), Some(TOKEN)),
            TOKEN
        ),
        Err(StatusCode::FORBIDDEN)
    );
    assert_eq!(
        check_request(&headers("evil.com:3000", None, Some(TOKEN)), TOKEN),
        Err(StatusCode::FORBIDDEN)
    );
}

#[test]
fn env_status_never_exposes_private_key() {
    let pk = "0x4c0883a69102937d6231471b5dbb6204fe5129617082792ae468d01a3f362318";
    let st = env_status(pk, "0xfunder");
    assert!(st.private_key_set);
    assert_eq!(
        st.signer_address.as_deref(),
        Some("0x2c7536E3605D9C16a7a3D7b1898e529396a65c23")
    );
    let json = serde_json::to_string(&st).unwrap();
    assert!(!json.contains(&pk[2..]), "private key leaked: {json}");

    let unset = env_status("", "0xfunder");
    assert!(!unset.private_key_set);
    assert!(unset.signer_address.is_none());
}
