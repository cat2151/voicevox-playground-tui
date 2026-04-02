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

To use this application, you need to start the VOICEVOX engine.

1. Download and install [VOICEVOX](https://voicevox.hiroshiba.jp/)
2. Start the VOICEVOX engine (this will launch an HTTP server on port 50021)

```bash
<your VOICEVOX directory>/vv-engine/run
```

If `VOICEVOX` or `mascot-render-server` is installed in a non-default location, you can set
their paths in `config.toml` under the app data directory used by this app
(`dirs::data_local_dir()/voicevox-playground-tui`, for example
`AppData/Local/voicevox-playground-tui` on Windows or
`~/.local/share/voicevox-playground-tui` on Linux):

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
| `--clipboard` | Read clipboard content line by line and exit (does not add to history.txt) |

## Key Bindings

| Key | Action |
|------|------|
| `i` | INSERT mode (edit current line) |
| `Enter` / `Esc` | Return from INSERT mode to NORMAL mode |
| `j` / `↓` | Move down one line → Autoplay |
| `k` / `↑` | Move up one line → Autoplay |
| `o` | Insert empty line below current line and enter INSERT mode |
| `O` | Insert empty line above current line and enter INSERT mode |
| `dd` | Cut line |
| `p` | Paste line (below current line) |
| `P` | Paste line (above current line) |
| `"+p` | Paste clipboard content below current line |
| `"+P` | Paste clipboard content above current line |
| `Enter` / `Space` | Manually play current line |
| `v` | Enter intonation editing mode |
| `zm` | Fold (hide lines with leading spaces) |
| `zr` | Unfold |
| `gt` | Move to next tab |
| `gT` | Move to previous tab |
| `:tabnew` | Create new tab |
| `q` | Quit (save history) |

- NORMAL mode key bindings are Vim-like
- INSERT mode key bindings are Emacs-like

## Specifications

- When moving the cursor, if a cache exists, play instantly; otherwise, asynchronously fetch from VOICEVOX and autoplay after completion.
- History is saved/loaded to `C:/Users/<your-name>/AppData/Local/voicevox-playground-tui/history.txt`
- Cache is in-memory (disappears when process ends)

## Update
- To update, run `vpt update` or execute the same installation command.

```
vpt update
```

```
cargo install --force --git https://github.com/cat2151/voicevox-playground-tui
```

## Future Plans

- Load a text file to the end via command-line arguments
- Generate WAV files for each line of history.txt via command-line arguments
- Hot reload history.txt by checking its timestamp every second for updates

## Notes

- Why a native application?
    - Do one thing well. I wanted a small application that does one small thing well.
        - However, there are no plans to implement standard I/O to make it part of a toolchain (out of scope).
            - Nowadays, it's sometimes smoother to implement desired functionalities directly with an LLM and iterate on UX verification cycles, rather than focusing on standard I/O.
    - Clipboard. The inspiration came from quickly implementing clipboard playback in Python via Claude chat, then brainstorming in Obsidian while playing it back, and when I casually threw that idea to Claude chat, this application came together quickly.
    - Easy OS integration. Easy local WAV file saving (future). Easy playback from clipboard (future).

- Readme Draft
    - A lightweight editor for quickly editing and playing multiple lines of dialogue.
    - More details
        - Zundamon at the speed of thought
        - Vim-like operations for editing and playback at the speed of thought.
        - Line-oriented, simple specifications.

- Name Ideas
    - voicevox-playground-tui
        - Pros: The content can be changed arbitrarily.
        - Cons: Like `voicevox-playground`, it doesn't clearly explain what it does.
            - It's unclear how it differs from `voicevox-playground`.
                - There's a risk of misunderstanding.
                    - People might mistakenly think it only allows editing one line of dialogue and has favorites, etc.
    - voicevox-text-editor
        - Pros: It's clear what it does.
            - Cons: It doesn't convey that it's line-oriented.

## Goals
- Claude: Demonstrate that a local native Rust Zundamon client can be easily realized with Claude chat (demonstrated).
- Sound playback: Maintain the experience of hearing sound after launching, entering insert mode, typing, and pressing ESC. If a bug prevents this, prioritize fixing that bug.
- Dialogue: Provide an experience where users can quickly write multiple lines of dialogue in a TUI, play them, and experiment.
- Line-oriented: Each line of dialogue data is self-contained. This maintains a simple specification by not affecting other lines.

## Non-Goals (Out of Scope)
- Standard I/O: Implementing standard I/O to make it usable as part of a toolchain.
- Automation: Neatly batch-exporting WAV files. File names with sequential numbers, speaker, style, and the beginning of the sentence (filename normalized). Intelligently and automatically selecting the appropriate folder and audio format for export, without specific user operations. Completely preventing data loss of valuable files. Completely preventing the accumulation of unnecessary large WAV files.
- Integration: A full-fledged integrated environment. A definitive all-in-one solution that covers VOICEVOX dialogue WAV generation, integration with video editing, and video distribution.
- Singing (like the VOICEVOX editor)
- DAW
- Advanced features: Editing features superior to or equivalent to the VOICEVOX editor.
- Vim: Advanced text editing features superior to or equivalent to Vim.
- Plugins: Vim plugin integration.
- GUI: GUI implementation using Tauri. Advanced visualization superior to or equivalent to the browser-based voicevox-playground.
