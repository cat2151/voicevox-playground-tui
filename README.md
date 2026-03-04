# voicevox-playground-tui

Write a line, play it immediately - Zundamon at the speed of thought.

## Requirements

- [VOICEVOX](https://voicevox.hiroshiba.jp/)
- Rust

## Installation

```
cargo install --force --git https://github.com/cat2151/voicevox-playground-tui
```

## Server

To use it, please start the VOICEVOX engine.

1. Download and install [VOICEVOX](https://voicevox.hiroshiba.jp/)
2. Start the VOICEVOX engine (this will launch an HTTP server on port 50021)

```bash
<your VOICEVOX directory>/vv-engine/run
```

## Usage

```
vpt
```

## Keybindings

| Key | Action |
|-----|--------|
| `i` | INSERT mode (edit current line) |
| `Enter` / `Esc` | Return from INSERT mode to NORMAL mode |
| `j` / `↓` | Move down one line → Autoplay |
| `k` / `↑` | Move up one line → Autoplay |
| `o` | Insert empty line below and enter INSERT mode |
| `dd` | Cut line |
| `p` | Paste line |
| `Enter` / `Space` | Manually play current line |
| `q` | Quit (save history) |

- NORMAL mode keybindings are Vim-like
- INSERT mode keybindings are Emacs-like

## Specifications

- When moving the cursor, if a cache exists, play immediately; otherwise, asynchronously fetch from VOICEVOX and autoplay upon completion.
- History is saved to/loaded from `C:/Users/<your-name>/AppData/Local/voicevox-playground-tui/history.txt`.
- Cache is in-memory (deleted when the process exits).

## Future Plans

- Play clipboard content via command-line argument and exit.
- Load a text file to the end via command-line argument.
- Generate .wav files for each line of `history.txt` via command-line argument.
- Check `history.txt` timestamp every second and hot-reload if updated.

## Notes

- Why a native application?
    - Do one thing well. I wanted a small application that does one small thing well.
        - However, there are no plans to implement standard I/O to make it part of a toolchain (out of scope).
            - Nowadays, it can sometimes be smoother to directly implement desired features using LLMs, like this application, and iterate on UX verification.
    - Clipboard: The impetus was that I could quickly implement clipboard playback in Python using Claude chat. While playing it back, I was brainstorming in Obsidian, and when I casually fed that to Claude chat, this was immediately created.
    - Easy OS integration. Easy local .wav file saving (future). Easy playback from clipboard (future).

- Readme draft
    - A lightweight editor for quickly editing and playing multiple lines of dialogue.
    - More details
        - Zundamon at the speed of thought.
        - Edit and play at the speed of thought with simple Vim-like operations.
        - Line-oriented simple specification.

- Naming ideas
    - `voicevox-playground-tui`
        - Pros: Content can be arbitrarily changed.
        - Cons: Like `voicevox-playground`, it doesn't explain what it does.
            - It's unclear how it differs from `voicevox-playground`.
                - Could easily be misunderstood.
                    - E.g., "This one also only edits one line and has favorites, right?" Such misunderstandings could occur.
    - `voicevox-text-editor`
        - Pros: Easy to understand what it does.
            - Shortcoming: Doesn't convey that it's line-oriented.

## Goals

- Claude: To demonstrate (and have demonstrated) that a local native Rust Zundamon client can be easily implemented with Claude chat.
- Sound playback: Maintain the experience of being able to play sound by launching, typing in insert mode, and pressing ESC. If a bug prevents this, prioritize fixing that bug.
- Dialogue: Provide an experience where users can quickly write, play, and experiment with multiple lines of dialogue in a TUI.
- Line-oriented: Each line completes a piece of dialogue data. Maintain a simple specification by not affecting other lines.

## Non-Goals (Out of Scope)

- Standard I/O: Implementing standard I/O to make it usable as part of a toolchain.
- Automation: Nicely batch-exporting .wav files. File names would include a serial number, speaker, style, and the beginning of the text (normalized). Intelligently and automatically choosing the appropriate folder and audio format for export, without specific user operations. Completely preventing data loss that could lead to valuable files being lost. Completely preventing the accumulation of unnecessary large numbers of .wav files.
- Integration: A full-fledged integrated environment. A definitive all-in-one solution that covers everything from VOICEVOX dialogue .wav generation and integration with video editing to video distribution.
- Singing (like the VOICEVOX editor).
- DAW.
- Advanced features: Advanced editing capabilities equivalent to or surpassing the VOICEVOX editor.
- Vim: Advanced text editing capabilities equivalent to or surpassing Vim.
- Plugins: Vim plugin integration.
- GUI: GUI conversion using Tauri. Advanced visualization capabilities equivalent to or surpassing the browser-based `voicevox-playground`.