use std::path::Path;

#[cfg(target_os = "windows")]
use std::process::Command;

#[derive(Debug)]
pub enum WindowsActionError {
    #[cfg_attr(not(windows), allow(dead_code))]
    Io(std::io::Error),
    #[cfg(target_os = "windows")]
    Windows(windows::core::Error),
    #[cfg_attr(windows, allow(dead_code))]
    Unsupported(&'static str),
}

impl std::fmt::Display for WindowsActionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(err) => write!(f, "io error: {}", err),
            #[cfg(target_os = "windows")]
            Self::Windows(err) => write!(f, "win32 error: {}", err),
            Self::Unsupported(msg) => write!(f, "unsupported: {}", msg),
        }
    }
}

impl std::error::Error for WindowsActionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            #[cfg(target_os = "windows")]
            Self::Windows(err) => Some(err),
            Self::Unsupported(_) => None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum SystemAction {
    VolumeMute,
    VolumeUp,
    VolumeDown,
    #[cfg_attr(not(windows), allow(dead_code))]
    VolumeSet(u8),
    Sleep,
    Shutdown,
    Restart,
    Lock,
}

#[cfg(target_os = "windows")]
pub fn open_path(path: &Path) -> Result<(), WindowsActionError> {
    let mut cmd = Command::new("cmd");
    cmd.args([
        "/C",
        "start",
        "",
        &format!("\"{}\"", path.to_string_lossy()),
    ]);
    run_detached(&mut cmd)
}

#[cfg(not(target_os = "windows"))]
pub fn open_path(_path: &Path) -> Result<(), WindowsActionError> {
    Err(WindowsActionError::Unsupported(
        "open path is only supported on Windows",
    ))
}

#[cfg(target_os = "windows")]
pub fn launch(app: &str) -> Result<(), WindowsActionError> {
    let mut cmd = Command::new("cmd");
    cmd.args(["/C", "start", "", &format!("\"{}\"", app)]);
    run_detached(&mut cmd)
}

#[cfg(not(target_os = "windows"))]
pub fn launch(_app: &str) -> Result<(), WindowsActionError> {
    Err(WindowsActionError::Unsupported("launch requires Windows"))
}

#[cfg(target_os = "windows")]
pub fn execute_system(action: SystemAction) -> Result<(), WindowsActionError> {
    match action {
        SystemAction::Sleep => suspend_system(),
        SystemAction::Shutdown => {
            let mut cmd = Command::new("shutdown");
            cmd.args(["/s", "/t", "0"]);
            run_detached(&mut cmd)
        }
        SystemAction::Restart => {
            let mut cmd = Command::new("shutdown");
            cmd.args(["/r", "/t", "0"]);
            run_detached(&mut cmd)
        }
        SystemAction::Lock => lock_workstation(),
        SystemAction::VolumeMute => send_volume_key(0xAD),
        SystemAction::VolumeDown => send_volume_key(0xAE),
        SystemAction::VolumeUp => send_volume_key(0xAF),
        SystemAction::VolumeSet(level) => set_master_volume(level),
    }
}

#[cfg(not(target_os = "windows"))]
pub fn execute_system(_action: SystemAction) -> Result<(), WindowsActionError> {
    Err(WindowsActionError::Unsupported(
        "system controls available only on Windows",
    ))
}

#[cfg(target_os = "windows")]
fn run_detached(cmd: &mut Command) -> Result<(), WindowsActionError> {
    cmd.spawn().map(|_| ()).map_err(WindowsActionError::Io)
}

#[cfg(target_os = "windows")]
fn send_volume_key(vk_code: u8) -> Result<(), WindowsActionError> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        keybd_event, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    };

    unsafe {
        keybd_event(vk_code, 0, KEYBD_EVENT_FLAGS(0), 0);
        keybd_event(vk_code, 0, KEYEVENTF_KEYUP, 0);
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn set_master_volume(level: u8) -> Result<(), WindowsActionError> {
    use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
    use windows::Win32::Media::Audio::{
        eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator,
    };
    use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};

    unsafe {
        let _guard = ComGuard::new()?;
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .map_err(WindowsActionError::Windows)?;
        let device = enumerator
            .GetDefaultAudioEndpoint(eRender, eConsole)
            .map_err(WindowsActionError::Windows)?;
        let endpoint: IAudioEndpointVolume = device
            .Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None)
            .map_err(WindowsActionError::Windows)?;
        let scalar = (level.min(100) as f32) / 100.0;
        endpoint
            .SetMasterVolumeLevelScalar(scalar, std::ptr::null())
            .map_err(WindowsActionError::Windows)?;
    }
    Ok(())
}

#[cfg(target_os = "windows")]
struct ComGuard;

#[cfg(target_os = "windows")]
impl ComGuard {
    fn new() -> Result<Self, WindowsActionError> {
        unsafe {
            windows::Win32::System::Com::CoInitializeEx(
                None,
                windows::Win32::System::Com::COINIT_MULTITHREADED,
            )
            .ok()
            .map_err(WindowsActionError::Windows)?;
        }
        Ok(Self)
    }
}

#[cfg(target_os = "windows")]
impl Drop for ComGuard {
    fn drop(&mut self) {
        unsafe {
            windows::Win32::System::Com::CoUninitialize();
        }
    }
}

#[cfg(target_os = "windows")]
fn suspend_system() -> Result<(), WindowsActionError> {
    use windows::Win32::Foundation::BOOLEAN;
    use windows::Win32::System::Power::SetSuspendState;

    unsafe {
        if SetSuspendState(BOOLEAN(0), BOOLEAN(0), BOOLEAN(0)).as_bool() {
            Ok(())
        } else {
            Err(last_os_error())
        }
    }
}

#[cfg(target_os = "windows")]
fn lock_workstation() -> Result<(), WindowsActionError> {
    use windows::Win32::System::Shutdown::LockWorkStation;

    unsafe { LockWorkStation().map_err(WindowsActionError::Windows) }
}

#[cfg(target_os = "windows")]
fn last_os_error() -> WindowsActionError {
    WindowsActionError::Windows(windows::core::Error::from_win32())
}
