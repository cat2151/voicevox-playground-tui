# voicevox-playground-tui

Write a line, play it instantly - Zundamon at the speed of thought -

## Features
- Text-to-speech
- Desktop mascot integration: [mascot-render-server](https://github.com/cat2151/mascot-render-server/blob/main/README.ja.md)
    - Displays Sakamoto Ahiru's standing character illustration ZIP files directly as a desktop mascot and makes it speak.

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
2. Start the VOICEVOX engine (this will launch an HTTP server on port 50021).

```bash
<your VOICEVOX directory>/vv-engine/run
```

If `VOICEVOX` or `mascot-render-server` are located in non-default directories, you can configure their paths in the `config.toml` file found in the application's data directory (e.g., `dirs::data_local_dir()/voicevox-playground-tui`, which on Windows would be `AppData/Local/voicevox-playground-tui`, and on Linux `~/.local/share/voicevox-playground-tui`).

```toml
voicevox_path = "<your voicevox path>"
voicevox_nemo_path = "<your voicevox nemo path>"
mascot_render_server_path = "<your mascot-render-server path>"
```

## Execution

```
vpt
```

## CLI Options

| Option | Action |
|-----------|------|
| `--clipboard` | Reads each line of the clipboard content aloud and exits (does not add to history.txt) |

## Keybindings

| Key | Action |
|------|------|
| `i` | INSERT mode (edit current line) |
| `Enter` / `Esc` | Return from INSERT mode to NORMAL mode |
| `j` / `↓` | Move down one line → Autoplay |
| `k` / `↑` | Move up one line → Autoplay |
| `o` | Insert an empty line below and enter INSERT mode |
| `O` | Insert an empty line above and enter INSERT mode |
| `dd` | Cut line |
| `p` | Paste line (below current line) |
| `P` | Paste line (above current line) |
| `"+p` | Paste clipboard content below current line |
| `"+P` | Paste clipboard content above current line |
| `Enter` / `Space` | Manually play current line |
| `v` | Enter intonation editing mode |
| `zm` | Fold (hide lines starting with spaces) |
| `zr` | Unfold |
| `gt` | Move to next tab |
| `gT` | Move to previous tab |
| `:tabnew` | Create new tab |
| `q` | Quit (save history) |

- NORMAL mode keybindings are Vim-like
- INSERT mode keybindings are Emacs-like

## Specifications

- When moving the cursor, if a cache exists, play immediately; otherwise, asynchronously fetch from VOICEVOX and autoplay upon completion.
- History is saved to and loaded from `C:/Users/<your-name>/AppData/Local/voicevox-playground-tui/history.txt`.
- Cache is in-memory (disappears when the process exits).

## Updates
- To update, run `vpt update` or execute the same command as for installation.

```
vpt update
```

```
cargo install --force --git https://github.com/cat2151/voicevox-playground-tui
```

## Future Plans

- Load text files at the end via command-line arguments
- Generate WAV files for each line in history.txt via command-line arguments
- Hot-reload history.txt if updated, checking timestamp every second

## Notes

- Why a native application?
    - Do one thing well. I wanted a small-scale application that does one small thing well.
        - However, there are no plans to implement standard I/O to make it part of a toolchain (out of scope).
            - Nowadays, it can sometimes be smoother to directly implement desired features using LLMs and iterate on UX validation, rather than relying on traditional toolchains.
    - Clipboard: The inspiration came when I quickly implemented clipboard playback in Python using Claude chat, then brainstormed in Obsidian while playing it back, and when I casually threw that idea to Claude chat, this application quickly came to be.
    - Easy OS integration. Easy local WAV file saving (future). Easy playback from clipboard (future).

- Readme Draft Ideas
    - A lightweight editor for quickly editing and playing multiple lines of dialogue.
    - More details:
        - Zundamon at the speed of thought
        - Edit and play at the speed of thought with simple Vim-like operations.
        - Line-oriented, simple specifications.

- Naming Ideas
    - `voicevox-playground-tui`
        - Pros: The content can be arbitrarily changed.
        - Cons: Like `voicevox-playground`, it doesn't clearly describe what it does.
            - It's unclear how it differs from `voicevox-playground`.
                - Potentially misleading.
                    - Misconceptions like "Does this also only edit one line and have favorites?" could arise.
    - `voicevox-text-editor`
        - Pros: Clearly indicates what it does.
            - Cons: Doesn't convey that it's line-oriented.

## Goals
- Claude: Demonstrate that a local native Rust Zundamon client can be easily realized with Claude chat (demonstrated).
- Sound playback: Maintain the experience of hearing sound by launching the app, entering text in insert mode, and pressing ESC. If a bug prevents this, prioritize fixing that bug.
- Dialogue: Provide an experience for quickly writing multiple lines of dialogue, playing them, and experimenting in a TUI.
- Line-oriented: Each line of dialogue data is self-contained. Maintain a simple specification by ensuring lines do not affect each other.

## Out of Scope / Non-Goals
- Standard I/O: Implementing standard I/O to make it usable as part of a toolchain.
- Automation: Nicely batch exporting WAV files. Filenames with sequential numbers, speaker, style, and the beginning of the text (filename normalized). Intelligently and automatically exporting to the appropriate folder, choosing the correct audio format, without requiring specific user operations. Completely preventing data loss of valuable files. Completely preventing the accumulation of unnecessary large WAV files.
- Integration: A full-fledged integrated environment. A definitive all-in-one solution that can handle everything from VOICEVOX dialogue WAV generation and video editing integration to video distribution.
- Singing (like the VOICEVOX editor)
- DAW
- Advanced features: Advanced editing capabilities equivalent to or surpassing the VOICEVOX editor.
- Vim: Advanced text editing capabilities equivalent to or surpassing Vim.
- Plugins: Becoming a Vim plugin.
- GUI: GUI implementation using Tauri. Advanced visualization equivalent to or surpassing the browser-based voicevox-playground.