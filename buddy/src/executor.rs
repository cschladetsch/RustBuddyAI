use crate::{
    config::Config,
    intent::Intent,
    windows_api::{self, SystemAction, WindowsActionError},
};
pub struct CommandExecutor<'a> {
    config: &'a Config,
}

impl<'a> CommandExecutor<'a> {
    pub fn new(config: &'a Config) -> Self {
        Self { config }
    }

    pub fn execute(&self, intent: &Intent) -> Result<ExecutionResult, ExecutionError> {
        match intent {
            Intent::OpenFile { target, .. } => self.open_target(target),
            Intent::OpenApp { target, .. } => self.launch_target(target),
            Intent::System { target, .. } => self.run_system(target),
            Intent::Answer { response, .. } => {
                Ok(ExecutionResult::Answer(response.clone()))
            }
            Intent::Unknown { .. } => Err(ExecutionError::UnknownIntent),
        }
    }

    fn open_target(&self, key: &str) -> Result<ExecutionResult, ExecutionError> {
        let path = self
            .config
            .files
            .get(key)
            .ok_or_else(|| ExecutionError::MissingMapping(key.to_string()))?;
        let resolved = if path.is_absolute() {
            path.clone()
        } else {
            std::env::current_dir()
                .map_err(ExecutionError::Io)?
                .join(path)
        };
        windows_api::open_path(&resolved).map_err(ExecutionError::Windows)?;
        Ok(ExecutionResult::Action(format!("Opened {}", key)))
    }

    fn launch_target(&self, key: &str) -> Result<ExecutionResult, ExecutionError> {
        let command = self
            .config
            .applications
            .get(key)
            .ok_or_else(|| ExecutionError::MissingMapping(key.to_string()))?;
        windows_api::launch(command).map_err(ExecutionError::Windows)?;
        Ok(ExecutionResult::Action(format!("Launched {}", key)))
    }

    fn run_system(&self, target: &str) -> Result<ExecutionResult, ExecutionError> {
        let action = parse_system_action(target)?;
        windows_api::execute_system(action).map_err(ExecutionError::Windows)?;
        Ok(ExecutionResult::Action(format!("Executed {}", target)))
    }
}

fn parse_system_action(target: &str) -> Result<SystemAction, ExecutionError> {
    match target {
        "volume_mute" => Ok(SystemAction::VolumeMute),
        "volume_up" => Ok(SystemAction::VolumeUp),
        "volume_down" => Ok(SystemAction::VolumeDown),
        "sleep" => Ok(SystemAction::Sleep),
        "shutdown" => Ok(SystemAction::Shutdown),
        "restart" => Ok(SystemAction::Restart),
        "lock" => Ok(SystemAction::Lock),
        action if action.starts_with("volume_set") => {
            let digits: String = action.chars().filter(|c| c.is_ascii_digit()).collect();
            let level = digits.parse::<u8>().unwrap_or(50);
            Ok(SystemAction::VolumeSet(level))
        }
        other => Err(ExecutionError::UnsupportedSystemAction(other.to_string())),
    }
}

#[derive(Debug)]
pub enum ExecutionError {
    MissingMapping(String),
    Windows(WindowsActionError),
    UnknownIntent,
    UnsupportedSystemAction(String),
    Io(std::io::Error),
}

#[derive(Debug)]
pub enum ExecutionResult {
    Action(String),
    Answer(String),
}

impl std::fmt::Display for ExecutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingMapping(key) => write!(f, "no mapping for key '{}'", key),
            Self::Windows(err) => write!(f, "windows action failed: {}", err),
            Self::UnknownIntent => write!(f, "intent classified as unknown"),
            Self::UnsupportedSystemAction(action) => {
                write!(f, "unsupported system action '{}'", action)
            }
            Self::Io(err) => write!(f, "io error: {}", err),
        }
    }
}

impl std::error::Error for ExecutionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Windows(err) => Some(err),
            Self::Io(err) => Some(err),
            _ => None,
        }
    }
}
