---
status: accepted
---

# farcli ships Microsoft's newer hidden terminal and does not use the one built into Windows

On Windows a Target app has to run on a hidden terminal (ConPTY), and Windows has one built in. That one gives a Half-space and each vowel mark a cell of its own, while Claude Code, Codex and farcli's Hidden screen give them none. It also draws the app's screen again in its own words, so farcli read the hidden terminal's miscounted picture, not what the app wrote: in the recordings of ticket #3, half of the reply rows holding a Half-space arrived damaged, and Claude Code's fullscreen display degraded as a whole. So farcli carries Microsoft's newer hidden terminal (`conpty.dll` and `OpenConsole.exe`, the pair Windows Terminal itself uses, from Microsoft's package `Microsoft.Windows.Console.ConPTY`) and runs the Target app on that.

A test on 2026-11-24 with version 1.25.261109001 confirmed it. The same bytes Claude Code and Codex wrote for an Input box holding Half-spaces came through it unchanged, and came through the built-in one with a stray border, lost spaces and garbled words. It passes on the app's output as written, counts Half-spaces and vowel marks as no cell, and delivers a large burst of keys whole, where the built-in one hands the app fragments of key codes as text.

The two files are carried inside `farcli.exe`, so farcli is still one file to download. In a Terminal it unpacks them, with Microsoft's licence beside them, to `%LOCALAPPDATA%\farcli\hidden-terminal\<version>-x64`, uses what lies there only if it is byte for byte what it carries, and keeps the files from changing while they are in use. When that fails, farcli says so in one line and uses the built-in hidden terminal.

## Considered Options

- **Keep the built-in hidden terminal and make the Hidden screen count as it does.** Rejected: that removes only farcli's share of the damage. A row the app padded with spaces and a row it placed by column need opposite corrections, and nothing on the screen tells them apart.
- **Ship the two files beside `farcli.exe`.** Rejected while unpacking works: the user would rather have one file, and a copy that lost its neighbours would fall back without the user knowing why. It remains the way out if unpacking proves unreliable.
- **Ask the user to run farcli only inside a Terminal that brings its own newer hidden terminal.** Rejected: farcli starts its own hidden terminal whatever the Terminal around it uses.
- **Read Logical text from somewhere other than the screen** (Claude Code's transcript, its screen reader display). Rejected for now: it leaves Codex and every plain program on the damaged path, and farcli works from the Hidden screen (ADR 0001).

## Consequences

- farcli carries about 1.2 MB of Microsoft's code, under Microsoft's MIT licence and not farcli's. `NOTICE` and `assets/hidden-terminal` say so, and the licence text travels with every unpacked copy.
- A newer release of the package has to be taken by hand: new files, new checksums in the notice, a new version in the code, and the recordings made again.
- farcli writes two programs into the user's own folders the first time it runs in a Terminal. A PC whose rules forbid running programs from there gets the built-in hidden terminal and the one-line notice. Piped, farcli unpacks nothing.
- Only the x64 files are carried. On any other processor farcli uses the built-in hidden terminal.
- What farcli reads changed, so recordings name the hidden terminal that produced them, and those made on the built-in one do not stand in for the shipped one.
- The shipped hidden terminal sends nothing after a resize; the Hidden screen rearranges its own picture and the Target app draws again. It also passes the app's questions about the Terminal on to farcli, where the built-in one answered some of them itself.
- On the built-in hidden terminal the Hidden screen still gives a Half-space no cell, so the damage described in #3 remains there.
