# farcli

Read and type Farsi in Claude Code CLI and Codex CLI on Windows. farcli runs the app on a hidden terminal and sends your Terminal the Farsi already joined, right to left and right-aligned.

## Install

1. Install Rust (https://rustup.rs).
2. `cargo build --release`
3. Put `target\release\farcli.exe` somewhere on your PATH.

## Use

```
farcli claude [arguments]
farcli codex  [arguments]
farcli <any program>
farcli setup
```

- The first run installs the Vazir Code font and adds it, for Farsi only, to Windows Terminal and VS Code (settings are backed up first). For WezTerm it prints the lines to add to your config.
- **F12** switches the correction off and on.
- Plain `claude` and `codex` are never changed.

## What works

- Farsi in replies of both apps: paragraphs, mixed Farsi/English, lists, tables, numbers such as `33 x 20`. Code stays left to right.
- Typing Farsi in Claude Code's Input box: joined, right-aligned, cursor in place, arrows move the way they look, Backspace/Delete, Half-space, paste.
- Multi-line paste and the mouse wheel reach the app.

## Not done yet

- Typing in Codex's Input box is shown as Codex draws it.
- Your own sent message in the conversation is shown as the app draws it.
- WSL, macOS and Linux.
- Copying Farsi with the mouse pastes in screen order.

## Licence

MIT. The bundled Vazir Code font and Microsoft's hidden terminal files keep their own licences; see `NOTICE`.
