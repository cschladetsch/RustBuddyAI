use crate::config::Config;
use reqwest::Client;
use serde::{Deserialize, Serialize};

pub struct IntentClient {
    client: Client,
    endpoint: String,
    model: String,
}

impl IntentClient {
    pub fn new(config: &Config) -> Self {
        let timeout = config.deepseek_timeout();
        let client = Client::builder()
            .timeout(timeout)
            .build()
            .expect("failed to build HTTP client");
        Self {
            client,
            endpoint: config.deepseek.endpoint.clone(),
            model: config.deepseek.model.clone(),
        }
    }

    pub async fn infer_intent(
        &self,
        transcription: &str,
        config: &Config,
    ) -> Result<IntentResponse, IntentError> {
        if transcription.trim().is_empty() {
            return Ok(IntentResponse::unknown());
        }

        let prompt = build_prompt(transcription, config);
        let payload = ChatRequest {
            model: &self.model,
            messages: vec![ChatMessage {
                role: "user",
                content: prompt,
            }],
            stream: false,
        };

        let response = self
            .client
            .post(&self.endpoint)
            .json(&payload)
            .send()
            .await
            .map_err(IntentError::Request)?
            .error_for_status()
            .map_err(IntentError::Http)?
            .json::<ChatResponse>()
            .await
            .map_err(IntentError::Response)?;

        let content = response
            .message
            .as_ref()
            .map(|msg| msg.content.trim())
            .unwrap_or_default();
        parse_intent(content)
    }
}

fn build_prompt(transcription: &str, config: &Config) -> String {
    let files = config.file_keys().join(", ");
    let apps = config.app_keys().join(", ");
    let systems = config.system_actions().join(", ");
    format!(
        "You interpret voice commands.\nUser said: \"{transcription}\"\nFILES: [{files}]\nAPPS: [{apps}]\nSYSTEM: [{systems}]\nReturn JSON only (no markdown, no code fences) with keys action, target, confidence.",
        transcription = transcription,
        files = files,
        apps = apps,
        systems = systems
    )
}

fn parse_intent(raw: &str) -> Result<IntentResponse, IntentError> {
    let cleaned = raw.trim();
    let cleaned = cleaned
        .strip_prefix("```json")
        .or_else(|| cleaned.strip_prefix("```"))
        .unwrap_or(cleaned)
        .strip_suffix("```")
        .unwrap_or(cleaned)
        .trim();
    let parsed: RawIntent = serde_json::from_str(cleaned).map_err(|err| IntentError::InvalidFormat {
        raw: raw.to_string(),
        err,
    })?;
    Ok(parsed.into())
}

#[derive(Debug, Clone, Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<ChatMessage<'a>>,
    stream: bool,
}

#[derive(Debug, Clone, Serialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: String,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    message: Option<ChatResponseMessage>,
}

#[derive(Debug, Deserialize)]
struct ChatResponseMessage {
    content: String,
}

#[derive(Debug, Clone, Copy)]
pub enum IntentAction {
    Open,
    Launch,
    System,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct IntentResponse {
    pub action: IntentAction,
    pub target: Option<String>,
    pub confidence: f32,
}

impl IntentResponse {
    fn unknown() -> Self {
        Self {
            action: IntentAction::Unknown,
            target: None,
            confidence: 0.0,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct RawIntent {
    action: Option<String>,
    target: Option<String>,
    confidence: Option<serde_json::Value>,
}

impl From<RawIntent> for IntentResponse {
    fn from(raw: RawIntent) -> Self {
        let action = match raw
            .action
            .as_deref()
            .unwrap_or_default()
            .to_lowercase()
            .as_str()
        {
            "open" => IntentAction::Open,
            "launch" => IntentAction::Launch,
            "system" => IntentAction::System,
            _ => IntentAction::Unknown,
        };
        let confidence = match raw.confidence {
            Some(serde_json::Value::Number(num)) => num.as_f64().unwrap_or(0.0) as f32,
            Some(serde_json::Value::String(s)) => match s.to_lowercase().as_str() {
                "high" => 0.9,
                "medium" => 0.6,
                "low" => 0.3,
                _ => 0.0,
            },
            Some(serde_json::Value::Bool(val)) => if val { 1.0 } else { 0.0 },
            _ => 0.0,
        };
        Self {
            action,
            target: raw.target,
            confidence,
        }
    }
}

#[derive(Debug)]
pub enum IntentError {
    Request(reqwest::Error),
    Http(reqwest::Error),
    Response(reqwest::Error),
    InvalidFormat { raw: String, err: serde_json::Error },
}

impl std::fmt::Display for IntentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Request(err) => write!(f, "request failed: {}", err),
            Self::Http(err) => write!(f, "HTTP error: {}", err),
            Self::Response(err) => write!(f, "failed parsing response: {}", err),
            Self::InvalidFormat { raw, err } => {
                write!(f, "invalid intent payload '{}': {}", raw, err)
            }
        }
    }
}

impl std::error::Error for IntentError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Request(err) | Self::Http(err) | Self::Response(err) => Some(err),
            Self::InvalidFormat { err, .. } => Some(err),
        }
    }
}
