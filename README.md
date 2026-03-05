# voicevox-playground-tui

Write dialogue, play instantly - Zundamon at the speed of thought -

## Requirements

- [VOICEVOX](https://voicevox.hiroshiba.jp/)
- Rust

## Installation

```
cargo install --force --git https://github.com/cat2151/voicevox-playground-tui
```

## Server

To use this, please start the VOICEVOX engine.

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
| `o` | Insert a blank line below and enter INSERT mode |
| `dd` | Cut line |
| `p` | Paste line |
| `Enter` / `Space` | Manually play current line |
| `q` | Quit (save history) |

- Keybindings in NORMAL mode are Vim-like
- Keybindings in INSERT mode are Emacs-like

## Features

- When moving the cursor, if a cache exists, play immediately; otherwise, asynchronously fetch from VOICEVOX and autoplay upon completion.
- History is saved and loaded at `C:/Users/<your-name>/AppData/Local/voicevox-playground-tui/history.txt`
- Cache is in-memory (disappears when the process ends)

## Update
- To update, simply run the same command as for installation.

```
cargo install --force --git https://github.com/cat2151/voicevox-playground-tui
```

## Future Plans

- Play clipboard content via command-line argument and exit
- Load a text file to the end via command-line argument
- Generate a WAV file for each line in `history.txt` via command-line argument
- Check `history.txt` timestamp every second and hot-reload if updated

## Notes

- Why a native application?
    - Do one thing well. I wanted a small-scale application that does one small thing well.
        - However, there's no intention to implement standard I/O to make it part of a toolchain (out of scope).
            - Nowadays, it can sometimes be smoother to directly implement desired functionalities using an LLM and iterate on UX validation, much like with this application, rather than integrating it into a standard toolchain.
- Clipboard. The catalyst was being able to quickly implement clipboard playback with Python via Claude chat. While playing that back and brainstorming in Obsidian, I casually threw it into Claude chat, and this application was immediately created.
- Easy OS integration. Easy local WAV file saving (future). Easy playback from clipboard (future).

- README draft
    - A lightweight editor for quickly editing and playing multiple lines of dialogue.
    - More details:
        - Zundamon at the speed of thought
        - Edit and play at the speed of thought with simple Vim-like operations
        - Line-oriented simple specifications

- Naming ideas
    - voicevox-playground-tui
        - Pros: The content can be arbitrarily changed.
        - Cons: Like voicevox-playground, it doesn't clearly explain what it does.
            - It's unclear how it differs from voicevox-playground.
                - This could lead to misunderstandings.
                    - Like, "this one also only edits one line of dialogue and has favorites, right?"
                    - Such misunderstandings could occur.
    - voicevox-text-editor
        - Pros: It's clear what it does.
        - Cons: It doesn't convey that it's line-oriented.

## Goals
- Claude. To demonstrate (and has demonstrated) that a local native Rust Zundamon client can be easily realized with Claude chat.
- Sound playback. To maintain the experience that launching the app, typing in INSERT mode, and pressing ESC results in sound. If a bug prevents this, prioritizing its fix will be considered.
- Dialogue. To provide an experience where users can quickly write and play multiple lines of dialogue in a TUI for fun.
- Line-oriented. Dialogue data is self-contained within a single line. This maintains a simple specification by not affecting other lines.

## Not Goals (Out of Scope)
- Standard I/O. Implementing standard I/O to make it usable as part of a toolchain.
- Automation. Nicely batch exporting WAV files. Filenames would be sequential, including speaker, style, and the beginning of the text (normalized filename). Intelligently and automatically exporting to the appropriate folder with the correct audio format without any specific user action. Completely preventing data loss of valuable files. Completely preventing the accumulation of unnecessary large WAV files.
- Integration. A full-fledged integrated environment. A definitive all-in-one solution that covers everything from VOICEVOX dialogue WAV generation and integration with video editing, to video distribution.
- Singing (like the VOICEVOX Editor)
- DAW (Digital Audio Workstation)
- Advanced features. Advanced editing functionalities equivalent to or surpassing the VOICEVOX Editor.
- Vim. Advanced text editing functionalities equivalent to or surpassing Vim.
- Plugins. Becoming a Vim plugin.
- GUI. GUI implementation using Tauri. Advanced visualization equivalent to or surpassing the browser-based voicevox-playground.