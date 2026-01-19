# Buddy

A local, privacy-first voice assistant for Windows. Control your PC with natural language - open files, launch apps, manage volume, and execute system commands.

**Core Principle:** Everything stays on your machine. No cloud. No telemetry. No paid APIs.

## Status

**Current Version:** 0.1.0-alpha (MVP)  
**Target:** Windows 10/11 (native binary)  
**Development:** WSL2 + cross-compilation

## Features

- 🎤 **Voice Commands** - Natural language control via Blue Yeti or similar quality mic
- 🤖 **AI-Powered Intent** - Local DeepSeek interprets what you want
- 🔒 **Privacy First** - All processing happens locally, nothing leaves your machine
- ⚙️ **Config-Driven** - Define your files, apps, and commands in TOML
- 🔊 **Audio Feedback** - Simple confirmation sounds or TTS responses
- ⚡ **Hotkey Activated** - Press key combo, speak command, done (wake word coming in v0.2)

## Quick Start

```bash
# In WSL2
rustup target add x86_64-pc-windows-gnu
cargo build --target x86_64-pc-windows-gnu --release

# Run on Windows
./target/x86_64-pc-windows-gnu/release/buddy.exe
```

Press configured hotkey (default: `Ctrl+Alt+B`), speak your command, wait for confirmation.

## Example Commands

```
"Open my details"           → Opens details.md
"Open Chrome"              → Launches Chrome browser  
"Open my resume"           → Opens resume.docx
"Shoosh"                   → Mutes volume
"Volume up"                → Increases volume
"Sleep"                    → Puts computer to sleep
"What can you do?"         → Lists available commands
```

## Architecture

```mermaid
flowchart TD
    A[Hotkey Press] --> B[Invoke Windows Speech Recognition]
    B --> C[Native Transcription]
    C --> D[Load Config]
    D --> E[Send to DeepSeek<br/>Local API]
    E --> F[Parse Intent]
    F --> G{Action Type?}
    G -->|File| H[Open File]
    G -->|App| I[Launch App]
    G -->|System| J[Execute Command]
    H --> K[Audio Feedback]
    I --> K
    J --> K
    K --> L[Return to Listening]
```

## System Flow

```mermaid
sequenceDiagram
    participant User
    participant Buddy
    participant WSR as Windows Speech
    participant DS as DeepSeek Local
    participant OS as Windows

    User->>Buddy: Press Ctrl+Alt+B
    Buddy->>WSR: Start Listening
    User->>WSR: "Open my resume"
    WSR->>Buddy: "open my resume"
    Buddy->>DS: Intent Request + Config
    DS->>Buddy: {"action": "open", "target": "resume"}
    Buddy->>OS: Open resume.docx
    OS->>Buddy: Success
    Buddy->>User: 🔊 "Ok"
```

## Configuration

### config.toml

```toml
[audio]
# How long Buddy waits for you to start speaking (seconds)
capture_duration_secs = 3

[hotkey]
# Trigger combination to start listening
key = "ctrl+alt+b"

[feedback]
# Audio feedback mode: "sound", "tts", or "both"
mode = "tts"
success_sound = "assets/success.wav"  # optional
error_sound = "assets/error.wav"      # optional
tts_voice = "default"                 # Windows SAPI voice

[deepseek]
# Local DeepSeek API endpoint
endpoint = "http://localhost:11434/api/chat"
model = "deepseek-r1:latest"
timeout_secs = 5

[transcription]
# Optional Windows speech settings
language_tag = "en-US"
topic_hint = "voice commands"
initial_silence_timeout_ms = 3000
end_silence_timeout_ms = 1200

# File mappings - "open X" commands
[files]
details = "C:/Users/YourName/Documents/details.md"
resume = "C:/Users/YourName/Documents/resume.docx"
contacts = "C:/Users/YourName/Documents/contacts.txt"

# Application mappings - "open/launch X" commands
[applications]
chrome = "chrome"
firefox = "firefox"
vscode = "code"
terminal = "wt"  # Windows Terminal

# System actions - available commands
[system]
volume_mute = true
volume_up = true
volume_down = true
volume_set = true  # "set volume to 50"
sleep = true
shutdown = true
restart = true
lock = true
```

## Dependencies

### Core
- **windows-rs** - Windows Speech Recognition + OS APIs
- **reqwest** - HTTP client for DeepSeek API
- **serde** - Config parsing and JSON handling
- **tokio** - Async runtime
- **toml** - Config file parsing

### System Integration
- **global-hotkey** - Hotkey registration
- **tts** - Text-to-speech (Windows SAPI)
- **rodio** - Audio playback for feedback sounds

## Setup Instructions

### 1. Install Rust and Targets

```bash
# In WSL2
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup target add x86_64-pc-windows-gnu

# Install MinGW cross-compiler
sudo apt update
sudo apt install -y mingw-w64
```

### 2. Enable Windows Speech Recognition

- On Windows, open **Settings → Time & Language → Speech**
- Make sure "Speech language" is installed (e.g., English (United States))
- Grant microphone access to desktop apps (Settings → Privacy & security → Microphone)
- Nothing to download — Buddy uses the built-in recognizer

### 3. Setup DeepSeek Local

```bash
# Install Ollama (or your preferred local LLM runner)
# On Windows: Download from https://ollama.ai

# Pull DeepSeek model
ollama pull deepseek-r1:latest

# Verify it's running
curl http://localhost:11434/api/tags
```

### 4. Create Config

```bash
# Copy example config
cp config.example.toml config.toml

# Edit with your paths
vim config.toml  # or nano, or whatever
```

### 5. Build and Run

```bash
# Build for Windows from WSL2
cargo build --target x86_64-pc-windows-gnu --release

# Copy to Windows accessible location
cp target/x86_64-pc-windows-gnu/release/buddy.exe /mnt/c/Users/YourName/buddy.exe

# Run from Windows (double-click or via cmd)
# Or run directly from WSL2:
/mnt/c/Users/YourName/buddy.exe
```

## Usage

1. **Start Buddy** - Run `buddy.exe` (consider adding to startup)
2. **Press Hotkey** - Default `Ctrl+Alt+B`
3. **Speak Command** - "Open my resume" or "Mute volume"
4. **Wait for Confirmation** - Audio feedback indicates success/failure

## DeepSeek Prompt Strategy

Buddy sends this context to DeepSeek for intent parsing:

```
You are a command interpreter for a voice assistant.

User said: "{transcription}"

Available capabilities:
FILES: {list of file keys from config}
APPS: {list of app keys from config}  
SYSTEM: {list of enabled system actions}

Respond with JSON only:
{
  "action": "open|launch|system|unknown",
  "target": "key from config or system command",
  "confidence": 0.0-1.0
}

Examples:
User: "open my details" → {"action": "open", "target": "details", "confidence": 0.95}
User: "launch chrome" → {"action": "launch", "target": "chrome", "confidence": 0.9}
User: "shoosh" → {"action": "system", "target": "volume_mute", "confidence": 0.85}
User: "what's the weather" → {"action": "unknown", "target": null, "confidence": 0.0}
```

## Project Structure

```
buddy/
├── src/
│   ├── main.rs              # Entry point, hotkey handling
│   ├── transcription.rs     # Windows Speech Recognition bridge
│   ├── intent.rs            # DeepSeek API client
│   ├── executor.rs          # Command execution
│   ├── feedback.rs          # Audio/TTS responses
│   ├── config.rs            # Config loading and validation
│   └── windows_api.rs       # Windows-specific system commands
├── assets/                  # Audio feedback files
├── config.toml             # User configuration
├── config.example.toml     # Template
└── Cargo.toml
```

## Development Workflow

```bash
# Standard dev cycle in WSL2
cargo check --target x86_64-pc-windows-gnu
cargo build --target x86_64-pc-windows-gnu
cargo test --target x86_64-pc-windows-gnu

# Quick test on Windows
/mnt/c/path/to/buddy.exe --verbose

# Watch for changes (optional)
cargo watch -x 'build --target x86_64-pc-windows-gnu'
```

## Roadmap

### v0.1 (MVP - Today's Goal)
- [x] Hotkey activation
- [x] Audio capture from Blue Yeti
- [x] Native Windows speech recognition
- [x] DeepSeek intent parsing
- [x] File opening
- [x] App launching
- [x] Volume control
- [x] Basic system commands
- [x] Audio feedback

### v0.2 (Wake Word)
- [ ] Continuous wake word detection ("Buddy")
- [ ] Always-on background service
- [ ] Low-power listening mode
- [ ] Improved noise handling

### v0.3 (Enhancement)
- [ ] Multi-step commands ("open chrome and go to youtube")
- [ ] Command history and learning
- [ ] Voice profile for better accuracy
- [ ] macOS and Linux support

### v1.0 (Production)
- [ ] Installer/setup wizard
- [ ] Windows Service integration
- [ ] Auto-update mechanism
- [ ] Comprehensive error recovery
- [ ] Performance optimization

## Troubleshooting

### Audio Not Captured
- Set the correct default recording device in Windows Sound settings
- Verify microphone privacy settings allow desktop apps
- Check hardware mute buttons (many USB mics have them)

### DeepSeek Not Responding
```bash
# Verify DeepSeek is running
curl http://localhost:11434/api/tags

# Check model is loaded
ollama list | grep deepseek
```

### Transcription Fails
- Make sure Windows Speech Recognition works in Settings
- Run the built-in speech training for better accuracy
- Check microphone privacy settings and disable "Allow desktop apps to access your microphone" off/on

### Commands Not Executing
```bash
# Run in verbose mode
buddy.exe --verbose

# Check logs (will be in same directory as exe)
cat buddy.log
```

### Cross-Compilation Issues
```bash
# Ensure MinGW is installed
sudo apt install mingw-w64

# Update linker in ~/.cargo/config
[target.x86_64-pc-windows-gnu]
linker = "x86_64-w64-mingw32-gcc"
```

## Performance Targets

- **Hotkey to Listening**: < 100ms
- **Transcription**: < 2s (Windows Speech Recognition)
- **DeepSeek Intent**: < 1s
- **Command Execution**: < 500ms
- **Total Latency**: < 4s from speech end to action

## Privacy & Security

- ✅ All processing is local - no cloud dependencies
- ✅ No telemetry or analytics
- ✅ No network access except localhost DeepSeek API
- ✅ Config file may contain sensitive paths - keep secure
- ⚠️ details.md with passwords - consider encryption at rest
- ⚠️ Voice commands are not authenticated - physical access = full access

## License

MIT - Do whatever you want with it.

## Credits

Built by Christian Schladetsch as a practical tool for voice-controlling Windows without cloud dependencies.

**Technologies:**
- [Windows Speech Recognition](https://learn.microsoft.com/windows/apps/design/input/speech-recognition) - Built-in transcription
- [DeepSeek](https://www.deepseek.com/) - Local LLM for intent parsing
- [Rust](https://www.rust-lang.org/) - Because memory safety matters for always-on services

---

**Note:** This is v0.1 without wake word detection. Press hotkey to activate. Wake word ("Buddy") coming in v0.2.
