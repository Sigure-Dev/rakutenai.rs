use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use chrono::Utc;
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE, AUTHORIZATION};
use futures_util::{StreamExt, SinkExt};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

use crate::crypto::{
    generate_device_id, get_signed_headers, get_signed_ws_url,
    BASE_URL, DEFAULT_AGENT_ID,
};
use crate::types::{
    ApiResponse, AuthTokens, ChatMode, CreateShareRequest, CreateShareResponse,
    CreateThreadRequest, FileUploadResponse, MessageContent, ShareData,
    StreamEvent, ThreadData, UploadedFile,
};

#[derive(Clone)]
pub struct User {
    pub device_id: String,
    inner: Arc<Mutex<UserInner>>,
    http: reqwest::Client,
}

struct UserInner {
    tokens: AuthTokens,
}

pub struct CreateThreadOptions {
    pub scenario_agent_id: Option<String>,
    pub title: Option<String>,
}

impl Default for CreateThreadOptions {
    fn default() -> Self {
        Self {
            scenario_agent_id: None,
            title: Some("新しいスレッド".to_string()),
        }
    }
}

pub struct UploadFileOptions {
    pub file_bytes: Vec<u8>,
    pub filename: String,
    pub mime_type: String,
    pub thread_id: Option<String>,
    pub is_image: bool,
}

impl User {
    pub async fn create() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let device_id = generate_device_id();
        let http = reqwest::Client::new();
        let tokens = Self::fetch_anonymous_token(&http, &device_id).await?;

        Ok(Self {
            device_id,
            inner: Arc::new(Mutex::new(UserInner { tokens })),
            http,
        })
    }

    async fn fetch_anonymous_token(
        http: &reqwest::Client,
        device_id: &str,
    ) -> Result<AuthTokens, Box<dyn std::error::Error + Send + Sync>> {
        let endpoint = "/api/v2/auth/anonymous";
        let url = format!("{}{}", BASE_URL, endpoint);
        let params = BTreeMap::new();
        let (timestamp, nonce, signature) = get_signed_headers("GET", endpoint, &params);

        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert("X-Platform", HeaderValue::from_static("WEB"));
        headers.insert("X-Country-Code", HeaderValue::from_static("JP"));
        headers.insert("Device-ID", HeaderValue::from_str(device_id)?);
        headers.insert("X-Timestamp", HeaderValue::from_str(&timestamp)?);
        headers.insert("X-Nonce", HeaderValue::from_str(&nonce)?);
        headers.insert("X-Signature", HeaderValue::from_str(&signature)?);

        let resp = http.get(&url).headers(headers).send().await?;
        if !resp.status().is_success() {
            return Err(format!("Auth HTTP Error: {}", resp.status()).into());
        }

        let expires_at = if let Some(expires_header) = resp.headers().get("expires") {
            if let Ok(s) = expires_header.to_str() {
                chrono::DateTime::parse_from_rfc2822(s)
                    .map(|dt| dt.timestamp_millis())
                    .unwrap_or_else(|_| Utc::now().timestamp_millis() + 3600_000)
            } else {
                Utc::now().timestamp_millis() + 3600_000
            }
        } else {
            Utc::now().timestamp_millis() + 3600_000
        };

        let result: ApiResponse<AuthTokens> = resp.json().await?;
        if result.code != "0" {
            return Err(result.message.unwrap_or_else(|| "Auth failed".into()).into());
        }

        let mut tokens = result.data;
        tokens.expires_at = expires_at;
        Ok(tokens)
    }

    pub async fn get_access_token(&self) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let mut inner = self.inner.lock().await;
        let now = Utc::now().timestamp_millis();
        if now >= inner.tokens.expires_at - 60_000 {
            let endpoint = "/api/v2/auth/refresh";
            let url = format!("{}{}", BASE_URL, endpoint);
            let params = BTreeMap::new();
            let (timestamp, nonce, signature) = get_signed_headers("POST", endpoint, &params);

            let mut headers = HeaderMap::new();
            headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
            headers.insert(AUTHORIZATION, HeaderValue::from_str(&format!("Bearer {}", inner.tokens.access_token))?);
            headers.insert("X-Platform", HeaderValue::from_static("WEB"));
            headers.insert("X-Country-Code", HeaderValue::from_static("JP"));
            headers.insert("Device-ID", HeaderValue::from_str(&self.device_id)?);
            headers.insert("X-Timestamp", HeaderValue::from_str(&timestamp)?);
            headers.insert("X-Nonce", HeaderValue::from_str(&nonce)?);
            headers.insert("X-Signature", HeaderValue::from_str(&signature)?);

            let body = serde_json::json!({ "refreshToken": inner.tokens.refresh_token });
            let resp = self.http.post(&url).headers(headers).json(&body).send().await?;
            if !resp.status().is_success() {
                return Err(format!("Refresh HTTP Error: {}", resp.status()).into());
            }

            let result: ApiResponse<AuthTokens> = resp.json().await?;
            if result.code != "0" {
                return Err(result.message.unwrap_or_else(|| "Refresh failed".into()).into());
            }

            inner.tokens = result.data;
            inner.tokens.expires_at = now + 3600_000;
        }

        Ok(inner.tokens.access_token.clone())
    }

    pub async fn create_thread(&self, opts: Option<CreateThreadOptions>) -> Result<Thread, Box<dyn std::error::Error + Send + Sync>> {
        let token = self.get_access_token().await?;
        let endpoint = "/api/v1/thread";
        let url = format!("{}{}", BASE_URL, endpoint);
        let params = BTreeMap::new();
        let (timestamp, nonce, signature) = get_signed_headers("POST", endpoint, &params);

        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(AUTHORIZATION, HeaderValue::from_str(&format!("Bearer {}", token))?);
        headers.insert("X-Platform", HeaderValue::from_static("WEB"));
        headers.insert("X-Country-Code", HeaderValue::from_static("JP"));
        headers.insert("Device-ID", HeaderValue::from_str(&self.device_id)?);
        headers.insert("X-Timestamp", HeaderValue::from_str(&timestamp)?);
        headers.insert("X-Nonce", HeaderValue::from_str(&nonce)?);
        headers.insert("X-Signature", HeaderValue::from_str(&signature)?);

        let options = opts.unwrap_or_default();
        let req_data = CreateThreadRequest {
            scenario_agent_id: options.scenario_agent_id.unwrap_or_else(|| DEFAULT_AGENT_ID.to_string()),
            title: options.title.or_else(|| Some("新しいスレッド".into())),
            source_language: None,
            target_language: None,
            shareable_link_id: None,
            multiple_thread_mode: None,
        };

        let resp = self.http.post(&url).headers(headers).json(&req_data).send().await?;
        if !resp.status().is_success() {
            let txt = resp.text().await.unwrap_or_default();
            return Err(format!("Create thread failed: {}", txt).into());
        }

        let result: ApiResponse<ThreadData> = resp.json().await?;
        if result.code != "0" {
            return Err(result.message.unwrap_or_else(|| "Create thread failed".into()).into());
        }

        let thread_data = result.data;
        Thread::connect(thread_data.id, self.clone()).await
    }

    pub async fn upload_file(
        &self,
        opts: UploadFileOptions,
    ) -> Result<UploadedFile, Box<dyn std::error::Error + Send + Sync>> {
        let token = self.get_access_token().await?;
        let endpoint = "/api/v1/files/upload";
        let url = format!("{}{}", BASE_URL, endpoint);
        let params = BTreeMap::new();
        let (timestamp, nonce, signature) = get_signed_headers("POST", endpoint, &params);

        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, HeaderValue::from_str(&format!("Bearer {}", token))?);
        headers.insert("X-Platform", HeaderValue::from_static("WEB"));
        headers.insert("X-Country-Code", HeaderValue::from_static("JP"));
        headers.insert("Device-ID", HeaderValue::from_str(&self.device_id)?);
        headers.insert("X-Timestamp", HeaderValue::from_str(&timestamp)?);
        headers.insert("X-Nonce", HeaderValue::from_str(&nonce)?);
        headers.insert("X-Signature", HeaderValue::from_str(&signature)?);

        let thread_id = opts.thread_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let json_meta = serde_json::json!({
            "type": if opts.is_image { "VISION_DATA" } else { "USER_DATA" },
            "agentId": DEFAULT_AGENT_ID,
            "threadId": thread_id,
        });

        let file_part = reqwest::multipart::Part::bytes(opts.file_bytes)
            .file_name(opts.filename)
            .mime_str(&opts.mime_type)?;

        let request_part = reqwest::multipart::Part::text(json_meta.to_string())
            .file_name("blob")
            .mime_str("application/json")?;

        let form = reqwest::multipart::Form::new()
            .part("file", file_part)
            .part("request", request_part);

        let resp = self.http.post(&url).headers(headers).multipart(form).send().await?;
        if !resp.status().is_success() {
            let txt = resp.text().await.unwrap_or_default();
            return Err(format!("Upload file failed: {}", txt).into());
        }

        let result: ApiResponse<FileUploadResponse> = resp.json().await?;
        if result.code != "0" {
            return Err(result.message.unwrap_or_else(|| "Upload failed".into()).into());
        }

        Ok(UploadedFile {
            file_id: result.data.file_id,
            file_url: result.data.file_url,
            file_name: result.data.original_filename,
            is_image: opts.is_image,
        })
    }
}

pub struct Thread {
    pub id: String,
    pub user: User,
    ws_stream: tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
}

impl Thread {
    pub async fn connect(id: String, user: User) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let token = user.get_access_token().await?;
        let ws_path = format!("/ws/v1/chat?deviceId={}", user.device_id);
        let signed_url = get_signed_ws_url(&ws_path, &token);

        let (ws_stream, _) = connect_async(signed_url).await?;

        Ok(Self {
            id,
            user,
            ws_stream,
        })
    }

    pub async fn from_shared(share_id: &str, user: User) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let token = user.get_access_token().await?;
        let endpoint = format!("/api/v1/share/{}", share_id);
        let url = format!("{}{}", BASE_URL, endpoint);
        let params = BTreeMap::new();
        let (timestamp, nonce, signature) = get_signed_headers("GET", &endpoint, &params);

        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, HeaderValue::from_str(&format!("Bearer {}", token))?);
        headers.insert("X-Platform", HeaderValue::from_static("WEB"));
        headers.insert("X-Country-Code", HeaderValue::from_static("JP"));
        headers.insert("Device-ID", HeaderValue::from_str(&user.device_id)?);
        headers.insert("X-Timestamp", HeaderValue::from_str(&timestamp)?);
        headers.insert("X-Nonce", HeaderValue::from_str(&nonce)?);
        headers.insert("X-Signature", HeaderValue::from_str(&signature)?);

        let resp = user.http.get(&url).headers(headers).send().await?;
        if !resp.status().is_success() {
            let txt = resp.text().await.unwrap_or_default();
            return Err(format!("Get share failed: {}", txt).into());
        }

        let res: ApiResponse<ShareData> = resp.json().await?;
        let share_data = res.data;
        let original_title = share_data.title.unwrap_or_else(|| "Shared Thread".to_string());

        // Create forked thread
        let fork_endpoint = "/api/v1/thread";
        let fork_url = format!("{}{}", BASE_URL, fork_endpoint);
        let (f_ts, f_nonce, f_sig) = get_signed_headers("POST", fork_endpoint, &params);

        let mut f_headers = HeaderMap::new();
        f_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        f_headers.insert(AUTHORIZATION, HeaderValue::from_str(&format!("Bearer {}", token))?);
        f_headers.insert("X-Platform", HeaderValue::from_static("WEB"));
        f_headers.insert("X-Country-Code", HeaderValue::from_static("JP"));
        f_headers.insert("Device-ID", HeaderValue::from_str(&user.device_id)?);
        f_headers.insert("X-Timestamp", HeaderValue::from_str(&f_ts)?);
        f_headers.insert("X-Nonce", HeaderValue::from_str(&f_nonce)?);
        f_headers.insert("X-Signature", HeaderValue::from_str(&f_sig)?);

        let req_data = CreateThreadRequest {
            scenario_agent_id: share_data.scenario_agent_id,
            title: Some(format!("Continue: {}", original_title)),
            source_language: None,
            target_language: None,
            shareable_link_id: Some(share_id.to_string()),
            multiple_thread_mode: Some(true),
        };

        let f_resp = user.http.post(&fork_url).headers(f_headers).json(&req_data).send().await?;
        let thread_res: ApiResponse<ThreadData> = f_resp.json().await?;
        Thread::connect(thread_res.data.id, user).await
    }

    pub async fn upload_file(
        &self,
        file_bytes: Vec<u8>,
        filename: String,
        mime_type: String,
        is_image: bool,
    ) -> Result<UploadedFile, Box<dyn std::error::Error + Send + Sync>> {
        self.user.upload_file(UploadFileOptions {
            file_bytes,
            filename,
            mime_type,
            thread_id: Some(self.id.clone()),
            is_image,
        }).await
    }

    pub async fn create_share(&self, message_ids: Vec<String>) -> Result<CreateShareResponse, Box<dyn std::error::Error + Send + Sync>> {
        let token = self.user.get_access_token().await?;
        let endpoint = "/api/v1/share/create";
        let url = format!("{}{}", BASE_URL, endpoint);
        let params = BTreeMap::new();
        let (timestamp, nonce, signature) = get_signed_headers("POST", endpoint, &params);

        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(AUTHORIZATION, HeaderValue::from_str(&format!("Bearer {}", token))?);
        headers.insert("X-Platform", HeaderValue::from_static("WEB"));
        headers.insert("X-Country-Code", HeaderValue::from_static("JP"));
        headers.insert("Device-ID", HeaderValue::from_str(&self.user.device_id)?);
        headers.insert("X-Timestamp", HeaderValue::from_str(&timestamp)?);
        headers.insert("X-Nonce", HeaderValue::from_str(&nonce)?);
        headers.insert("X-Signature", HeaderValue::from_str(&signature)?);

        let req = CreateShareRequest {
            thread_id: self.id.clone(),
            message_ids,
        };

        let resp = self.user.http.post(&url).headers(headers).json(&req).send().await?;
        let res: ApiResponse<CreateShareResponse> = resp.json().await?;
        if res.code != "0" {
            return Err(res.message.unwrap_or_else(|| "Create share failed".into()).into());
        }
        Ok(res.data)
    }

    pub async fn send_message<F>(
        &mut self,
        contents: Vec<MessageContent>,
        mode: ChatMode,
        mut on_event: F,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
    where
        F: FnMut(StreamEvent),
    {
        let user_message_id = uuid::Uuid::new_v4().to_string();
        let timestamp = Utc::now().timestamp_millis();

        let json_contents: Vec<serde_json::Value> = contents.into_iter().map(|c| match c {
            MessageContent::Text(text) => serde_json::json!({
                "contentType": "TEXT",
                "textData": {
                    "text": text
                }
            }),
            MessageContent::File(file) => {
                if file.is_image {
                    serde_json::json!({
                        "contentType": "INPUT_IMAGE",
                        "inputImageData": {
                            "src": file.file_url,
                            "resourceId": file.file_id
                        }
                    })
                } else {
                    serde_json::json!({
                        "contentType": "INPUT_FILE",
                        "inputFileData": {
                            "src": file.file_url,
                            "resourceId": file.file_id,
                            "name": file.file_name
                        }
                    })
                }
            }
        }).collect();

        let payload = serde_json::json!({
            "message": {
                "type": "CONVERSATION",
                "payload": {
                    "action": mode.as_action_str(),
                    "data": {
                        "chatRequestType": mode.as_action_str(),
                        "role": "user",
                        "userId": self.user.device_id,
                        "threadId": self.id,
                        "messageId": user_message_id,
                        "language": "ja",
                        "platform": "WEB",
                        "timestamp": timestamp,
                        "contents": json_contents,
                        "retry": false,
                        "debug": false,
                        "timezoneString": "Asia/Tokyo",
                        "countryCode": "JP",
                        "city": "Nerima",
                        "explicitSearch": "AUTO"
                    }
                },
                "metadata": {
                    "messageId": user_message_id,
                    "timestamp": timestamp
                }
            }
        });

        self.ws_stream.send(Message::Text(payload.to_string().into())).await?;

        let mut collected_message_ids = vec![user_message_id.clone()];

        while let Some(msg_result) = self.ws_stream.next().await {
            let msg = match msg_result {
                Ok(m) => m,
                Err(_) => {
                    on_event(StreamEvent::Disconnected);
                    return Ok(());
                }
            };
            if let Message::Text(text_str) = msg {
                let v: serde_json::Value = match serde_json::from_str(&text_str) {
                    Ok(val) => val,
                    Err(_) => continue,
                };

                let ws_type = v.pointer("/webSocket/type").and_then(|s| s.as_str()).unwrap_or("");
                match ws_type {
                    "ACK" => {
                        on_event(StreamEvent::Ack {
                            message_id: user_message_id.clone(),
                        });
                    }
                    "CONVERSATION" => {
                        let action = v.pointer("/webSocket/payload/action").and_then(|s| s.as_str()).unwrap_or("");
                        let status = v.pointer("/webSocket/payload/data/chatResponseStatus").and_then(|s| s.as_str()).unwrap_or("");

                        if action == "AI_ANSWER" || action == "EVENT" {
                            if status == "APPEND" {
                                if let Some(prog) = v.pointer("/webSocket/payload/data/progressEvent") {
                                    on_event(StreamEvent::ToolCallDetail { data: prog.clone() });
                                } else if let Some(contents) = v.pointer("/webSocket/payload/data/contents").and_then(|c| c.as_array()) {
                                    for item in contents {
                                        let c_type = item.get("contentType").and_then(|s| s.as_str()).unwrap_or("");
                                        if c_type == "TEXT" {
                                            let t = item.pointer("/textData/text").and_then(|s| s.as_str()).unwrap_or("");
                                            if action == "EVENT" {
                                                if t == "思考中..." {
                                                    on_event(StreamEvent::ReasoningStart);
                                                }
                                            } else {
                                                on_event(StreamEvent::TextDelta { text: t.to_string() });
                                            }
                                        } else if c_type == "SUMMARY_TEXT" {
                                            let t = item.pointer("/textData/text").and_then(|s| s.as_str()).unwrap_or("");
                                            on_event(StreamEvent::ReasoningDelta { text: t.to_string() });
                                        } else if c_type == "OUTPUT_IMAGE" {
                                            if let Some(thumb) = item.pointer("/outputImageData/imageGens/0/thumbnail").and_then(|s| s.as_str()) {
                                                on_event(StreamEvent::ImageThumbnail { url: thumb.to_string() });
                                            }
                                            if let Some(prev) = item.pointer("/outputImageData/imageGens/0/preview").and_then(|s| s.as_str()) {
                                                on_event(StreamEvent::Image { url: prev.to_string() });
                                            }
                                        } else if c_type == "RESPONSE_METRICS" {
                                            let in_tok = item.pointer("/responseMetricsData/usage/inputTokens").and_then(|n| n.as_u64()).unwrap_or(0);
                                            let out_tok = item.pointer("/responseMetricsData/usage/outputTokens").and_then(|n| n.as_u64()).unwrap_or(0);
                                            on_event(StreamEvent::Usage {
                                                input_tokens: in_tok,
                                                output_tokens: out_tok,
                                            });
                                        }
                                    }
                                }
                            } else if status == "TOOL_CALL" {
                                if let Some(contents) = v.pointer("/webSocket/payload/data/contents") {
                                    on_event(StreamEvent::ToolCall { data: contents.clone() });
                                }
                            } else if status == "DONE" {
                                if let Some(ai_msg_id) = v.pointer("/webSocket/payload/data/messageId").and_then(|s| s.as_str()) {
                                    collected_message_ids.push(ai_msg_id.to_string());
                                }
                                on_event(StreamEvent::Done {
                                    message_ids: collected_message_ids,
                                });
                                return Ok(());
                            }
                        }
                    }
                    "NOTIFICATION" => {
                        if let Some(data) = v.pointer("/webSocket/payload/data") {
                            on_event(StreamEvent::Notification { data: data.clone() });
                        }
                    }
                    "ERROR" => {
                        let code = v.pointer("/webSocket/error/code").and_then(|s| s.as_str()).unwrap_or("unknown");
                        let message = v.pointer("/webSocket/error/message").and_then(|s| s.as_str()).unwrap_or("unknown error");
                        on_event(StreamEvent::Error {
                            code: code.to_string(),
                            message: message.to_string(),
                        });
                        return Ok(());
                    }
                    _ => {}
                }
            }
        }

        Ok(())
    }
}
