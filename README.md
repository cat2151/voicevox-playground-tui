# voicevox-playground-tui

Write a line, play it instantly - Zundamon at the speed of thought -

## Requirements

- [VOICEVOX](https://voicevox.hiroshiba.jp/) 
- Rust

## Installation

```
cargo install --force --git https://github.com/cat2151/voicevox-playground-tui
```

## Server

To use it, start the VOICEVOX engine.

1. Download and install [VOICEVOX](https://voicevox.hiroshiba.jp/)
2. Start the VOICEVOX engine (an HTTP server will launch on port 50021)

```bash
<your VOICEVOX directory>/vv-engine/run
```

## Usage

```
vpt
```

## Keybinds

| Key | Action |
|------|------|
| `i` | INSERT mode (edit current line) |
| `Enter` / `Esc` | Return from INSERT mode to NORMAL mode |
| `j` / `↓` | Move down a line → Auto-play |
| `k` / `↑` | Move up a line → Auto-play |
| `o` | Insert an empty line below and enter INSERT mode |
| `dd` | Cut line |
| `p` | Paste line |
| `Enter` / `Space` | Manually play current line |
| `q` | Quit (save history) |

- NORMAL mode keybinds are Vim-like
- INSERT mode keybinds are Emacs-like

## Features

- When moving the cursor, if a cache exists, play instantly; otherwise, asynchronously fetch from VOICEVOX and auto-play upon completion.
- History is saved/loaded to/from `C:/Users/<your-name>/AppData/Local/voicevox-playground-tui/history.txt`.
- Cache is in-memory (disappears when the process ends).

## Future Plans

- Play clipboard content via command-line argument and exit.
- Load a text file to the end via command-line argument.
- Generate WAV files for each line in `history.txt` via command-line argument.
- Hot-reload `history.txt` if updated, by checking its timestamp every second.
- Automatic updates (if enabled in settings): Upon startup, reference the GitHub repository, get the `main` branch's hash, and if it differs from local, run `cargo install`. During the final phase of `cargo install`'s build, the executable should error due to being locked. If that happens, launch a separate process, terminate the main application, run `cargo install` again to succeed the final phase, and then restart itself to complete the automatic update. This is the current assumption. Not yet verified. Planning to test it.

## Notes

- Why a native app?
    - Do one thing well. I wanted a small-scale app that does one small thing well.
        - However, there's no intention to implement standard I/O to make it part of a toolchain (out of scope).
            - Nowadays, it can be smoother to directly implement desired features using an LLM and iterate on UX verification, similar to this app.
    - Clipboard: The trigger was being able to quickly implement clipboard playback in Python via Claude chat, then brainstorming in Obsidian while playing it back, and when I casually threw that idea to Claude chat, this was quickly created.
    - Easy OS integration: Easy local WAV file saving (future). Easy playback from clipboard (future).

- README draft ideas
    - A lightweight editor for quickly editing and playing multiple lines of dialogue.
    - More details
        - Zundamon at the speed of thought
        - Simple Vim-like operations for editing and playback at the speed of thought.
        - Line-oriented, simple specifications.

- Name ideas
    - `voicevox-playground-tui`
        - Pro: The content can be changed arbitrarily with this name.
        - Con: Like `voicevox-playground`, it doesn't describe what the tool does.
            - It's unclear how it differs from `voicevox-playground`.
                - Potentially misleading (e.g., the misconception that 'this also only edits one line and has favorites').
    - `voicevox-text-editor`
        - Pro: Clearly indicates what it does.
            - Drawback: Doesn't convey its line-oriented nature.

## Goals

- Claude: To demonstrate that a local native Rust Zundamon client can be easily realized with Claude chat (demonstrated).
- Sound playback: To maintain the experience of hearing sound by launching the app, entering text in INSERT mode, and pressing ESC. If this fails due to a bug, prioritize fixing that bug.
- Dialogue: To provide an experience where users can quickly write, play, and experiment with multiple lines of dialogue in a TUI.
- Line-oriented: To maintain a simple design where each line's dialogue data is self-contained and does not affect other lines.

## Out of Scope

- Standard I/O: Implementing standard I/O to make it usable as part of a toolchain.
- Automation: Neatly exporting WAV files in batches, with sequential filenames including speaker, style, and the normalized beginning of the text. This would involve intelligently and automatically writing to the appropriate folder, selecting the correct audio format without specific user interaction, and completely preventing both data loss and the accumulation of unnecessary large numbers of WAV files.
- Integration: A full-fledged integrated environment. A definitive all-in-one solution that covers everything from VOICEVOX dialogue WAV generation and video editing integration to video distribution.
- Singing (like the VOICEVOX editor).
- DAW.
- Advanced features: Editing capabilities equivalent to or superior to the VOICEVOX editor.
- Vim: Advanced text editing features equivalent to or superior to Vim.
- Plugins: Becoming a Vim plugin.
- GUI: Creating a GUI with Tauri. Advanced visualization equivalent to or superior to the browser-based `voicevox-playground`.