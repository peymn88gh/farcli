# farcli

farcli lets a Farsi speaker read and type Farsi correctly inside AI coding tools that run in a terminal. It exists because those tools and most terminals lay out right-to-left text wrongly.

## Language

### Programs

**farcli**:
The command the user runs in a Terminal to start a Target app with correct Farsi reading and typing.

**Terminal**:
The window program the user opens to run commands, such as Windows Terminal, WezTerm or macOS Terminal.
_Avoid_: console, shell, terminal app

**Supported terminal**:
A Terminal on the named list that has been tested with farcli.

**Target app**:
A program that farcli runs and corrects. Claude Code CLI and Codex CLI are the primary Target apps; any other program is a bonus.
_Avoid_: terminal app, CLI, agent

**Input box**:
The part of a Target app where the user types a message.
_Avoid_: prompt, composer

**Agent output**:
The text a Target app's AI writes back to the user.

**Setup**:
The mandatory one-time step that gives every Supported terminal on a machine the Farsi font.

### Text

**Logical order**:
The order in which characters are typed and stored, first character first.

**Screen order**:
The left-to-right order in which characters sit on the screen.
_Avoid_: visual order

**Joining**:
Drawing Farsi letters in their connected forms.
_Avoid_: shaping, ligatures

**Paragraph direction**:
Whether a paragraph is laid out right-to-left or left-to-right. It is taken from the paragraph's first letter; code, file paths and commands are always left-to-right.
_Avoid_: alignment, base direction

**Half-space** (نیم‌فاصله):
The character that ends a letter's join without leaving a visible gap.
_Avoid_: ZWNJ, zero-width space

**Farsi font**:
The fixed-width font (Vazir Code) that a Terminal needs so that joined letters connect without gaps.
