---
status: accepted
---

# farcli is a wrapper that sends pre-joined letters in Screen order

Most Terminals either do no right-to-left layout (Windows Terminal) or do it per row in ways that scramble a Target app's screen, and Claude Code and Codex each handle Farsi differently per operating system. So farcli runs the Target app on a hidden screen, works out the layout itself, and sends the Terminal letters that are already joined and already in Screen order. This asks nothing of the Terminal beyond drawing one character per cell, which is why the same approach can work in PowerShell, WSL, macOS and Linux.

A rendering test on 2026-11-23 confirmed that Windows Terminal 1.25 and WezTerm 20240203 both draw such text correctly, and that the letters connect cleanly with the Vazir Code font.

## Considered Options

- **Configure a Terminal that does right-to-left layout itself (WezTerm).** Rejected: it ties the user to one Terminal, cannot fix typing in the Input box, and clashes with Claude Code's own reordering on Windows.
- **Adopt an existing right-to-left terminal program (RtlTerminal and its ports).** Rejected: the user wants to own the solution and keep using the Terminals they already open.
- **Build our own Terminal window program.** Rejected: it would not work inside the Terminals the user already opens.

## Consequences

- On Windows, Claude Code reorders Farsi itself with no off switch, so farcli has to undo that before doing its own layout. This depends on Claude Code's behaviour and can break when Claude Code updates.
- Text selected with the mouse is copied by the Terminal in Screen order with joined letters, which pastes as garbage elsewhere until farcli handles selection itself.
- A Terminal needs a fixed-width Farsi font, or the joined letters show gaps.
- A Terminal that reorders text itself and cannot be told to stop may fight farcli. Such a Terminal is listed as unsupported.
