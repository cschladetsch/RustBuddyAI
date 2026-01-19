use crate::config::HotkeyConfig;
use std::{
    fmt, ptr,
    sync::{
        atomic::{AtomicU32, Ordering},
        mpsc as std_mpsc,
    },
    thread,
};
#[cfg(target_os = "windows")]
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

#[cfg(target_os = "windows")]
use windows::Win32::{
    Foundation::{HWND, LPARAM, WPARAM},
    System::Threading::GetCurrentThreadId,
    UI::{
        Input::KeyboardAndMouse::{self, *},
        WindowsAndMessaging::{GetMessageW, PostThreadMessageW, MSG, WM_HOTKEY, WM_QUIT},
    },
};

#[cfg(target_os = "windows")]
static HOTKEY_ID: AtomicU32 = AtomicU32::new(1);

pub use platform::{HotkeyError, HotkeyListener};

#[cfg(target_os = "windows")]
mod platform {
    use super::*;
    use windows::core::Error as WinError;
    pub struct HotkeyListener {
        rx: UnboundedReceiver<()>,
        thread: Option<thread::JoinHandle<()>>,
        thread_id: u32,
    }

    impl HotkeyListener {
        pub fn new(cfg: &HotkeyConfig) -> Result<Self, HotkeyError> {
            let (modifiers, vk) = parse_hotkey(&cfg.key)?;
            let hotkey_id = super::HOTKEY_ID.fetch_add(1, Ordering::Relaxed);
            let (event_tx, event_rx) = mpsc::unbounded_channel();
            let (ready_tx, ready_rx) = std_mpsc::channel();

            let thread =
                thread::spawn(move || hotkey_worker(hotkey_id, modifiers, vk, event_tx, ready_tx));

            let ready = match ready_rx.recv().map_err(|_| HotkeyError::ThreadInit)? {
                Ok(data) => data,
                Err(err) => return Err(err),
            };

            Ok(Self {
                rx: event_rx,
                thread: Some(thread),
                thread_id: ready.thread_id,
            })
        }

        pub async fn wait(&mut self) -> Result<(), HotkeyError> {
            self.rx.recv().await.ok_or(HotkeyError::Channel)
        }
    }

    impl Drop for HotkeyListener {
        fn drop(&mut self) {
            unsafe {
                let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
            }
            if let Some(handle) = self.thread.take() {
                let _ = handle.join();
            }
        }
    }

    struct HotkeyReady {
        thread_id: u32,
    }

    fn hotkey_worker(
        hotkey_id: u32,
        modifiers: HOT_KEY_MODIFIERS,
        key: VIRTUAL_KEY,
        tx: UnboundedSender<()>,
        ready: std_mpsc::Sender<Result<HotkeyReady, HotkeyError>>,
    ) {
        unsafe {
            let thread_id = GetCurrentThreadId();
            let flags = modifiers | MOD_NOREPEAT;
            if let Err(err) = KeyboardAndMouse::RegisterHotKey(
                HWND(ptr::null_mut()),
                hotkey_id as i32,
                flags,
                key.0 as u32,
            ) {
                let _ = ready.send(Err(HotkeyError::Register(err)));
                return;
            }
            let _ = ready.send(Ok(HotkeyReady { thread_id }));

            let mut msg = MSG::default();
            loop {
                let status = GetMessageW(&mut msg, HWND(ptr::null_mut()), 0, 0);
                if status.0 <= 0 {
                    break;
                }
                if msg.message == WM_HOTKEY && msg.wParam == WPARAM(hotkey_id as usize) {
                    let _ = tx.send(());
                }
                if msg.message == WM_QUIT {
                    break;
                }
            }

            let _ = KeyboardAndMouse::UnregisterHotKey(HWND(ptr::null_mut()), hotkey_id as i32);
        }
    }

    fn parse_hotkey(hotkey: &str) -> Result<(HOT_KEY_MODIFIERS, VIRTUAL_KEY), HotkeyError> {
        let mut modifiers = HOT_KEY_MODIFIERS(0);
        let mut key = None;
        for token in hotkey.split('+') {
            let token = token.trim().to_lowercase();
            match token.as_str() {
                "ctrl" | "control" => modifiers |= MOD_CONTROL,
                "alt" => modifiers |= MOD_ALT,
                "shift" => modifiers |= MOD_SHIFT,
                "win" | "windows" => modifiers |= MOD_WIN,
                other => {
                    key = Some(
                        parse_key(other).ok_or_else(|| HotkeyError::Parse(other.to_string()))?,
                    );
                }
            }
        }
        let key = key.ok_or_else(|| HotkeyError::Parse("missing key".into()))?;
        Ok((modifiers, key))
    }

    fn parse_key(key: &str) -> Option<VIRTUAL_KEY> {
        Some(match key {
            "a" => VK_A,
            "b" => VK_B,
            "c" => VK_C,
            "d" => VK_D,
            "e" => VK_E,
            "f" => VK_F,
            "g" => VK_G,
            "h" => VK_H,
            "i" => VK_I,
            "j" => VK_J,
            "k" => VK_K,
            "l" => VK_L,
            "m" => VK_M,
            "n" => VK_N,
            "o" => VK_O,
            "p" => VK_P,
            "q" => VK_Q,
            "r" => VK_R,
            "s" => VK_S,
            "t" => VK_T,
            "u" => VK_U,
            "v" => VK_V,
            "w" => VK_W,
            "x" => VK_X,
            "y" => VK_Y,
            "z" => VK_Z,
            "0" => VK_0,
            "1" => VK_1,
            "2" => VK_2,
            "3" => VK_3,
            "4" => VK_4,
            "5" => VK_5,
            "6" => VK_6,
            "7" => VK_7,
            "8" => VK_8,
            "9" => VK_9,
            "space" => VK_SPACE,
            "enter" => VK_RETURN,
            "f1" => VK_F1,
            "f2" => VK_F2,
            "f3" => VK_F3,
            "f4" => VK_F4,
            "f5" => VK_F5,
            "f6" => VK_F6,
            "f7" => VK_F7,
            "f8" => VK_F8,
            "f9" => VK_F9,
            "f10" => VK_F10,
            "f11" => VK_F11,
            "f12" => VK_F12,
            "f13" => VK_F13,
            "f14" => VK_F14,
            "f15" => VK_F15,
            "f16" => VK_F16,
            "f17" => VK_F17,
            "f18" => VK_F18,
            "f19" => VK_F19,
            "f20" => VK_F20,
            "f21" => VK_F21,
            "f22" => VK_F22,
            "f23" => VK_F23,
            "f24" => VK_F24,
            _ => return None,
        })
    }

    #[derive(Debug)]
    pub enum HotkeyError {
        Parse(String),
        Register(WinError),
        Channel,
        ThreadInit,
    }

    impl fmt::Display for HotkeyError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::Parse(key) => write!(f, "invalid hotkey '{}'", key),
                Self::Register(err) => write!(f, "failed to register hotkey: {}", err),
                Self::Channel => write!(f, "hotkey event channel closed"),
                Self::ThreadInit => write!(f, "failed to initialize hotkey listener"),
            }
        }
    }

    impl std::error::Error for HotkeyError {}
}

#[cfg(not(target_os = "windows"))]
mod platform {
    use super::*;

    pub struct HotkeyListener {
        label: String,
    }

    impl HotkeyListener {
        pub fn new(cfg: &HotkeyConfig) -> Result<Self, HotkeyError> {
            Ok(Self {
                label: cfg.key.clone(),
            })
        }

        pub async fn wait(&mut self) -> Result<(), HotkeyError> {
            println!("Press Enter to simulate hotkey '{}'", self.label);
            let mut input = String::new();
            std::io::stdin()
                .read_line(&mut input)
                .map_err(HotkeyError::Interrupt)?;
            Ok(())
        }
    }

    #[derive(Debug)]
    pub enum HotkeyError {
        Interrupt(std::io::Error),
    }

    impl fmt::Display for HotkeyError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::Interrupt(err) => write!(f, "input interrupted: {}", err),
            }
        }
    }

    impl std::error::Error for HotkeyError {}
}
