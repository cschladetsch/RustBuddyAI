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
    ) -> Result<Intent, IntentError> {
        if transcription.trim().is_empty() {
            return Ok(Intent::Unknown { confidence: 0.0 });
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
        let intent = parse_intent(content)?;
        validate_intent_target(&intent, config)?;
        Ok(intent)
    }

    pub async fn wait_for_ready(&self) -> Result<(), IntentError> {
        let tags_endpoint = if self.endpoint.ends_with("/api/chat") {
            self.endpoint.replace("/api/chat", "/api/tags")
        } else {
            self.endpoint.clone()
        };
        self.client
            .get(&tags_endpoint)
            .send()
            .await
            .map_err(IntentError::Request)?
            .error_for_status()
            .map_err(IntentError::Http)?;

        Ok(())
    }
}

fn build_prompt(transcription: &str, config: &Config) -> String {
    let files = config.file_keys().join(", ");
    let apps = config.app_keys().join(", ");
    let systems = config.system_actions().join(", ");
    format!(
        "You interpret voice commands for a desktop assistant.\nUser said: \"{transcription}\"\nAvailable files: {files}\nAvailable apps: {apps}\nAvailable system actions: {systems}\nRules:\n- action must be one of: open_file, open_app, system, answer, unknown\n- use open_file/open_app/system only when the request matches an available key\n- for action=answer, provide a direct response text and set target to null\n- if unsure, use action=unknown and target=null\nExamples:\nInput: \"open my resume\" => {{\"action\":\"open_file\",\"target\":\"resume\",\"response\":null,\"confidence\":0.9}}\nInput: \"start chrome\" => {{\"action\":\"open_app\",\"target\":\"chrome\",\"response\":null,\"confidence\":0.8}}\nInput: \"turn volume down\" => {{\"action\":\"system\",\"target\":\"volume_down\",\"response\":null,\"confidence\":0.8}}\nInput: \"what is 2+3\" => {{\"action\":\"answer\",\"target\":null,\"response\":\"5\",\"confidence\":0.9}}\nReturn JSON only (no markdown, no code fences) with keys action, target, response, confidence.",
        transcription = transcription,
        files = files,
        apps = apps,
        systems = systems
    )
}

fn parse_intent(raw: &str) -> Result<Intent, IntentError> {
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

fn validate_intent_target(
    intent: &Intent,
    config: &Config,
) -> Result<(), IntentError> {
    match intent {
        Intent::OpenFile { target, .. } => {
            if !config.files.contains_key(target) {
                return Err(IntentError::UnknownTarget(target.to_string()));
            }
        }
        Intent::OpenApp { target, .. } => {
            if !config.applications.contains_key(target) {
                return Err(IntentError::UnknownTarget(target.to_string()));
            }
        }
        Intent::System { target, .. } => {
            if !config.system_actions().contains(&target.as_str()) {
                return Err(IntentError::UnknownTarget(target.to_string()));
            }
        }
        Intent::Answer { .. } | Intent::Unknown { .. } => {}
    }
    Ok(())
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
    OpenFile,
    OpenApp,
    System,
    Answer,
    Unknown,
}

#[derive(Debug, Clone)]
pub enum Intent {
    OpenFile { target: String, confidence: f32 },
    OpenApp { target: String, confidence: f32 },
    System { target: String, confidence: f32 },
    Answer { response: String, confidence: f32 },
    Unknown { confidence: f32 },
}

impl Intent {
    pub fn confidence(&self) -> f32 {
        match self {
            Self::OpenFile { confidence, .. }
            | Self::OpenApp { confidence, .. }
            | Self::System { confidence, .. }
            | Self::Answer { confidence, .. }
            | Self::Unknown { confidence, .. } => *confidence,
        }
    }

    pub fn action(&self) -> IntentAction {
        match self {
            Self::OpenFile { .. } => IntentAction::OpenFile,
            Self::OpenApp { .. } => IntentAction::OpenApp,
            Self::System { .. } => IntentAction::System,
            Self::Answer { .. } => IntentAction::Answer,
            Self::Unknown { .. } => IntentAction::Unknown,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct RawIntent {
    action: Option<String>,
    target: Option<String>,
    response: Option<String>,
    confidence: Option<serde_json::Value>,
}

impl From<RawIntent> for Intent {
    fn from(raw: RawIntent) -> Self {
        let action = match raw
            .action
            .as_deref()
            .unwrap_or_default()
            .to_lowercase()
            .as_str()
        {
            "open_file" => IntentAction::OpenFile,
            "open_app" => IntentAction::OpenApp,
            "system" => IntentAction::System,
            "answer" => IntentAction::Answer,
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
        match action {
            IntentAction::OpenFile => raw
                .target
                .map(|target| Self::OpenFile { target, confidence })
                .unwrap_or(Self::Unknown { confidence }),
            IntentAction::OpenApp => raw
                .target
                .map(|target| Self::OpenApp { target, confidence })
                .unwrap_or(Self::Unknown { confidence }),
            IntentAction::System => raw
                .target
                .map(|target| Self::System { target, confidence })
                .unwrap_or(Self::Unknown { confidence }),
            IntentAction::Answer => raw
                .response
                .map(|response| Self::Answer { response, confidence })
                .unwrap_or(Self::Unknown { confidence }),
            IntentAction::Unknown => Self::Unknown { confidence },
        }
    }
}

#[derive(Debug)]
pub enum IntentError {
    Request(reqwest::Error),
    Http(reqwest::Error),
    Response(reqwest::Error),
    InvalidFormat { raw: String, err: serde_json::Error },
    UnknownTarget(String),
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
            Self::UnknownTarget(target) => {
                write!(f, "unknown target '{}'", target)
            }
        }
    }
}

impl std::error::Error for IntentError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Request(err) | Self::Http(err) | Self::Response(err) => Some(err),
            Self::InvalidFormat { err, .. } => Some(err),
            Self::UnknownTarget(_) => None,
        }
    }
}
