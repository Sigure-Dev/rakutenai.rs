use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct ApiResponse<T> {
    pub code: String,
    pub message: Option<String>,
    pub data: T,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthTokens {
    #[serde(rename = "accessToken")]
    pub access_token: String,
    #[serde(rename = "refreshToken")]
    pub refresh_token: String,
    #[serde(rename = "idToken")]
    pub id_token: Option<String>,
    #[serde(rename = "userType")]
    pub user_type: Option<String>,
    #[serde(rename = "expiresAt", default)]
    pub expires_at: i64,
}

#[derive(Debug, Serialize, Default)]
pub struct CreateThreadRequest {
    #[serde(rename = "scenarioAgentId")]
    pub scenario_agent_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(rename = "sourceLanguage", skip_serializing_if = "Option::is_none")]
    pub source_language: Option<String>,
    #[serde(rename = "targetLanguage", skip_serializing_if = "Option::is_none")]
    pub target_language: Option<String>,
    #[serde(rename = "shareableLinkId", skip_serializing_if = "Option::is_none")]
    pub shareable_link_id: Option<String>,
    #[serde(rename = "multipleThreadMode", skip_serializing_if = "Option::is_none")]
    pub multiple_thread_mode: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadData {
    pub id: String,
    #[serde(rename = "threadId", default)]
    pub thread_id: Option<String>,
    #[serde(rename = "scenarioAgentId")]
    pub scenario_agent_id: String,
    pub title: String,
    #[serde(rename = "createdAt", default)]
    pub created_at: Option<i64>,
    #[serde(rename = "updatedAt", default)]
    pub updated_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadedFile {
    #[serde(rename = "fileId")]
    pub file_id: String,
    #[serde(rename = "fileUrl")]
    pub file_url: String,
    #[serde(rename = "fileName")]
    pub file_name: String,
    #[serde(rename = "isImage")]
    pub is_image: bool,
}

#[derive(Debug, Deserialize)]
pub struct FileUploadResponse {
    #[serde(rename = "originalFilename")]
    pub original_filename: String,
    #[serde(rename = "fileId")]
    pub file_id: String,
    #[serde(rename = "fileUrl")]
    pub file_url: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateShareRequest {
    #[serde(rename = "threadId")]
    pub thread_id: String,
    #[serde(rename = "messageIds")]
    pub message_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateShareResponse {
    #[serde(rename = "shareId")]
    pub share_id: String,
    #[serde(rename = "shareUrl")]
    pub share_url: String,
    pub title: String,
    #[serde(rename = "alreadyExists")]
    pub already_exists: bool,
    #[serde(rename = "fullShareUrl")]
    pub full_share_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareData {
    pub title: Option<String>,
    #[serde(rename = "shareId")]
    pub share_id: String,
    #[serde(rename = "shareUrl")]
    pub share_url: String,
    #[serde(rename = "scenarioAgentId")]
    pub scenario_agent_id: String,
}

#[derive(Debug, Clone)]
pub enum MessageContent {
    Text(String),
    File(UploadedFile),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatMode {
    UserInput,
    DeepThink,
    AiRead,
}

impl ChatMode {
    pub fn as_action_str(&self) -> &'static str {
        match self {
            ChatMode::UserInput => "USER_INPUT",
            ChatMode::DeepThink => "DEEP_THINK",
            ChatMode::AiRead => "AI_READ",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ImageGenData {
    pub thumbnail: String,
    pub preview: Option<String>,
}

#[derive(Debug, Clone)]
pub enum StreamEvent {
    Ack { message_id: String },
    ReasoningStart,
    ReasoningDelta { text: String },
    TextDelta { text: String },
    ImageThumbnail { url: String },
    Image { url: String },
    ToolCallDetail { data: serde_json::Value },
    ToolCall { data: serde_json::Value },
    Notification { data: serde_json::Value },
    Usage { input_tokens: u64, output_tokens: u64 },
    Done { message_ids: Vec<String> },
    Error { code: String, message: String },
    Disconnected,
}
