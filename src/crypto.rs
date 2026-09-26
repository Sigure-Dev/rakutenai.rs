use std::collections::BTreeMap;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::Utc;
use rand::Rng;
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

pub const BASE_URL: &str = "https://ai.rakuten.co.jp";
pub const WS_BASE_URL: &str = "wss://companion.ai.rakuten.co.jp";
pub const SECRET_KEY: &str = "4f0465bfea7761a510dda451ff86a935bf0c8ed6fb37f80441509c64328788c8";
pub const DEFAULT_AGENT_ID: &str = "6812e64f9dfaf301f7000001";

pub fn generate_device_id() -> String {
    let mut rng = rand::thread_rng();
    let chars: String = (0..6)
        .map(|_| {
            let idx = rng.gen_range(0..36);
            if idx < 10 {
                (b'0' + idx) as char
            } else {
                (b'a' + (idx - 10)) as char
            }
        })
        .collect();
    format!("{}-{}", Uuid::new_v4(), chars)
}

pub fn generate_nonce() -> String {
    Uuid::new_v4().to_string()
}

pub fn generate_signature(message: &str, secret: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .expect("HMAC can take key of any size");
    mac.update(message.as_bytes());
    let result = mac.finalize();
    let bytes = result.into_bytes();
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn get_signed_headers(
    method: &str,
    url_path: &str,
    params: &BTreeMap<String, String>,
) -> (String, String, String) {
    let timestamp = Utc::now().timestamp().to_string();
    let nonce = generate_nonce();

    let mut sorted_params = String::new();
    for (k, v) in params {
        sorted_params.push_str(k);
        sorted_params.push('=');
        sorted_params.push_str(v);
    }

    let raw_string = format!(
        "{}{}{}{}{}",
        method.to_uppercase(),
        url_path,
        sorted_params,
        timestamp,
        nonce
    );

    let signature = generate_signature(&raw_string, SECRET_KEY);
    (timestamp, nonce, signature)
}

pub fn get_signed_ws_url(path: &str, access_token: &str) -> String {
    let timestamp = Utc::now().timestamp().to_string();
    let nonce = generate_nonce();
    let method = "GET";

    let mut url = url::Url::parse(&format!("{}{}", WS_BASE_URL, path)).unwrap();
    url.query_pairs_mut()
        .append_pair("accessToken", access_token)
        .append_pair("platform", "WEB");

    let mut filtered_params = BTreeMap::new();
    for (k, v) in url.query_pairs() {
        if !k.to_lowercase().starts_with("x-") {
            filtered_params.insert(k.to_string(), v.to_string());
        }
    }

    let mut sorted_params = String::new();
    for (k, v) in &filtered_params {
        sorted_params.push_str(k);
        sorted_params.push('=');
        sorted_params.push_str(v);
    }

    let raw_string = format!(
        "{}{}{}{}{}",
        method,
        url.path(),
        sorted_params,
        timestamp,
        nonce
    );

    let signature = generate_signature(&raw_string, SECRET_KEY);

    url.query_pairs_mut()
        .append_pair("x-timestamp", &timestamp)
        .append_pair("x-nonce", &nonce)
        .append_pair("x-signature", &signature);

    url.to_string()
}
