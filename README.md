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

To use this, please start the VOICEVOX engine.

1. Download and install [VOICEVOX](https://voicevox.hiroshiba.jp/)
2. Start the VOICEVOX engine (an HTTP server will launch on port 50021)

```bash
<your VOICEVOX directory>/vv-engine/run
```

If VOICEVOX is installed in a custom location, set it in:

`C:/Users/<your-name>/AppData/Local/voicevox-playground-tui/config.toml`

```toml
voicevox_path = "<your voicevox path>"
voicevox_nemo_path = "<your voicevox nemo path>"
```

## Usage

```
vpt
```

## CLI Options

| Option | Action |
|-----------|------|
| `--clipboard` | Reads clipboard content line by line, speaks it, and exits (does not add to history.txt) |

## Keybindings

| Key | Action |
|------|------|
| `i` | INSERT mode (edit current line) |
| `Enter` / `Esc` | Return to NORMAL mode from INSERT mode |
| `j` / `↓` | Move to the line below → Auto-play |
| `k` / `↑` | Move to the line above → Auto-play |
| `o` | Insert a blank line below the current line and enter INSERT mode |
| `O` | Insert a blank line above the current line and enter INSERT mode |
| `dd` | Cut line |
| `p` | Paste line (below current line) |
| `P` | Paste line (above current line) |
| `"+p` | Paste clipboard content below the current line |
| `"+P` | Paste clipboard content above the current line |
| `Enter` / `Space` | Manually play current line |
| `v` | Enter intonation editing mode |
| `zm` | Fold (hide lines with leading spaces) |
| `zr` | Unfold |
| `gt` | Move to next tab |
| `gT` | Move to previous tab |
| `:tabnew` | Create new tab |
| `q` | Quit (save history) |

- NORMAL mode keybindings are Vim-like
- INSERT mode keybindings are Emacs-like

## Specifications

- When moving the cursor, if a cache exists, play immediately; otherwise, asynchronously fetch from VOICEVOX and auto-play upon completion.
- History is saved and loaded from `C:/Users/<your-name>/AppData/Local/voicevox-playground-tui/history.txt`
- Cache is in-memory (disappears when the process exits)

## Updating

- To update, simply run the same installation command.

```
cargo install --force --git https://github.com/cat2151/voicevox-playground-tui
```

## Future Plans

- Load a text file at the end via command-line arguments
- Generate WAV files for each line of history.txt via command-line arguments
- Hot reload history.txt if updated, checking timestamp every second

## Notes

- Why a native application?
    - Do one thing well. I wanted a small application that does one small thing well.
        - However, I don't intend to implement standard I/O to make it part of a toolchain (out of scope).
            - Nowadays, it's sometimes smoother to implement desired functionality directly into an LLM, like this app, and iterate on UX verification.
    - Clipboard. The impetus was realizing I could quickly implement clipboard playback with Python via Claude chat, then brainstorming in Obsidian while playing it back, and when I casually threw that idea to Claude chat, this application was quickly created.
    - Easy OS integration. Easy local WAV file saving (future). Easy playback from clipboard (future).

- README draft ideas
    - A lightweight editor for quickly editing and playing multiple lines of dialogue.
    - More details:
        - Zundamon at the speed of thought
        - Edit and play at the speed of thought with simple Vim-like operations
        - Line-oriented, simple specifications

- Naming ideas
    - voicevox-playground-tui
        - Pros: The content can be changed arbitrarily with this name.
        - Cons: Like voicevox-playground, it doesn't clearly describe what it does.
            - It's unclear how it differs from voicevox-playground.
                - Potentially misleading.
                    - "This also only lets you edit one line and has favorites, right?"
                        - Such misunderstandings could occur.
    - voicevox-text-editor
        - Pros: Clearly describes what it does.
            - Con: It doesn't convey that it's line-oriented.

## Goals

- Claude. Demonstrate that a local native Rust Zundamon client can be easily realized with Claude chat (demonstrated).
- Sound playback. Maintain the experience that sound plays when you start, type in insert mode, and press ESC. If a bug prevents this, prioritize fixing that bug.
- Dialogue. Provide an experience where users can quickly write and play multiple lines of dialogue in a TUI.
- Line-oriented. Dialogue data is self-contained within a single line. Maintain a simple specification by not affecting other lines.

## Non-Goals (Out of Scope)

- Standard I/O. Implementing standard I/O to make it usable as part of a toolchain.
- Automation. Nicely batching and exporting WAV files. File names with sequential numbers, speaker, style, and sentence beginning (normalized filename). Intelligently and automatically exporting to appropriate folders and audio formats without specific user action. Completely preventing data loss of valuable files. Completely preventing the accumulation of unnecessary large WAV files.
- Integration. A full-fledged integrated environment. A definitive all-in-one solution that can handle dialogue WAV generation with VOICEVOX, integration with video editing, and even video distribution.
- Singing (like the VOICEVOX editor).
- DAW (Digital Audio Workstation).
- Advanced Features. Advanced editing functions equivalent to or exceeding the VOICEVOX editor.
- Vim. Advanced text editing functions equivalent to or exceeding Vim.
- Plugins. Becoming a Vim plugin.
- GUI. GUI conversion using Tauri. Advanced visualization equivalent to or exceeding the browser-based voicevox-playground.
