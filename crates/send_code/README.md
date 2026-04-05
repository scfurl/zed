# SendCode for Zed

Send code from the editor to a terminal or external REPL. Inspired by the Sublime Text [SendCode](https://github.com/furlan-lab/SendCode) plugin.

## Features

- **Send to Zed's built-in terminal** or external terminals (Ghostty, iTerm, Terminal.app, cmux, tmux)
- **Language-aware block expansion** — automatically selects the right code block based on language semantics (Python, R, Julia)
- **Per-language target routing** — send R to Ghostty, Python to cmux, etc.
- **Bracketed paste mode** — multiline code is sent using bracketed paste to avoid auto-indentation issues in REPLs
- **Debug mode** — preview extracted code in a confirmation toast before sending

## Actions

| Action | Description |
|--------|-------------|
| `send_code::SendCode` | Send current selection, or auto-expand to block if no selection. Advances cursor past the block. For non-block-aware languages, sends the current line. On blank lines, sends Enter. |
| `send_code::SendCodeInPlace` | Same as `SendCode` but does not advance the cursor. |
| `send_code::SendLine` | Send the current line (or selection if highlighted). Advances cursor to the next line. On blank lines, sends Enter. |
| `send_code::SendFile` | Source the entire file. Uses language-specific source commands (R: `source()`, Python: `exec(open().read())`, Julia: `include()`). |
| `send_code::ChooseTarget` | Switch the active send target (not yet implemented). |

## Keybindings

No default keybindings are set. Add these to your Zed keymap (`Zed > Settings > Open Key Bindings`):

```json
[
  {
    "context": "Editor && mode == full",
    "bindings": {
      "cmd-enter": "send_code::SendLine",
      "shift-cmd-enter": "send_code::SendFile",
      "ctrl-enter": "send_code::SendCode"
    }
  }
]
```

> **Note**: `cmd-enter` overrides the default `editor::NewlineBelow` binding in editor context. Use a different key if you want to keep that behavior.

## Settings

Add to your `settings.json`:

```jsonc
{
  "send_code": {
    // Whether SendCode actions are enabled (default: true)
    "enabled": true,

    // Show a confirmation toast with code preview before sending (default: false)
    "debug": false,

    // Default target: "zed_terminal", "ghostty", "iterm", "terminal_app", "cmux", "tmux"
    "target": "zed_terminal",

    // Use bracketed paste mode for multiline sends (default: true)
    "bracketed_paste": true,

    // Chunk size for Ghostty AppleScript sends (default: 1000)
    "ghostty_chunk_size": 1000,

    // Chunk size for cmux CLI sends (default: 200)
    "cmux_chunk_size": 200,

    // Target a specific cmux surface/pane (default: null, uses focused surface)
    "cmux_surface": null,

    // Target a specific tmux pane, e.g. "session:window.pane" (default: null)
    "tmux_target": null,

    // Per-language target overrides
    "language_targets": {
      "R": "ghostty",
      "Python": "cmux",
      "Julia": "zed_terminal"
    }
  }
}
```

## Debug Mode

When `"debug": true`, each send action shows a toast notification displaying:
- Detected language
- Target terminal
- Bracketed paste status
- Text length and a preview (first 200 chars)

Click **"Send"** on the toast to confirm and send the code. Dismiss or ignore the toast to cancel. This is useful for diagnosing block expansion issues.

## Send Targets

### Zed Terminal (`zed_terminal`)

Sends code to the active terminal in Zed's terminal panel. With bracketed paste enabled, multiline code is wrapped in `\x1b[200~`/`\x1b[201~` escape sequences. Without bracketed paste, lines are sent one at a time with 50ms delays.

**Requirements**: A terminal must be open in the terminal panel.

### Ghostty (`ghostty`)

Sends code to Ghostty via AppleScript. Text is chunked into segments (default 1000 chars) with small delays to avoid stalling.

**Requirements**: Ghostty >= 1.3.0, macOS.

### cmux (`cmux`)

Sends code via the `cmux` CLI (`cmux send <text>`, `cmux send-key <key>`). Text is chunked into segments (default 200 chars). Supports targeting a specific surface with the `cmux_surface` setting.

**Requirements**: cmux installed. The sender checks `/opt/homebrew/bin/cmux` and `/usr/local/bin/cmux` if not in PATH.

### iTerm (`iterm`)

Sends code to iTerm2 via AppleScript (`write text` to current session). Text is chunked into 1000-char segments.

**Requirements**: iTerm2 >= 2.9, macOS.

### Terminal.app (`terminal_app`)

Sends code to macOS Terminal.app via AppleScript (`do script` in front window).

**Requirements**: macOS.

### tmux (`tmux`)

Sends code via `tmux set-buffer` + `tmux paste-buffer`. Text is chunked into 200-char segments. Supports targeting a specific pane with the `tmux_target` setting.

**Requirements**: tmux installed.

## Language-Aware Block Expansion

When `SendCode` is triggered with no selection, behavior depends on the language:

### Block-aware languages (Python, R, Julia)

The cursor position is expanded to a language-aware block. Comments are handled intelligently:

**Python**
1. **Jupytext cells**: If `# %%` cell markers are present, expands to the cell containing the cursor.
2. **Indentation**: Expands to the contiguous block at the same or deeper indentation level, including the parent statement (`def`, `class`, `if`, `for`, etc.) and any decorators (`@`).
3. **Comment handling**: Top-level comments (indent 0) are treated as boundaries. Comments inside function/class bodies are included as part of the block.

**R**
- **Pipe operators**: `%>%` (magrittr), `|>` (native pipe)
- **ggplot `+`**: Lines ending in `+` continue the expression
- **Bracket balancing**: Unmatched `(`, `[`, `{` extend the block forward
- **Comma/equals continuation**: Lines ending in `,` or `=`
- **Backslash continuation**: Lines ending in `\`
- **Comment handling**: Standalone comments are boundaries and won't bleed into adjacent code. Comments embedded inside bracketed expressions (e.g., inside `inner_join(...)`) are included as part of the expression. Cursor on a standalone comment sends just the comment block.

**Julia**
1. **Outline-based**: Uses tree-sitter outline queries to find the enclosing `function`, `struct`, `module`, etc.
2. **Keyword pairs**: Falls back to scanning for `function`/`end`, `begin`/`end`, `for`/`end`, etc. Comments inside blocks are skipped during keyword matching.
3. **Jupytext cells**: Supports `# %%` cell markers.

### All other languages

`SendCode` sends the **current line only** (same as `SendLine`). This includes shell (bash, zsh), plain text, Markdown, and any language without dedicated block expansion. This prevents the paragraph heuristic from grabbing too much context in non-structured languages.

### Markdown with language injection

In Markdown files, Zed's tree-sitter language injection means the cursor's detected language changes based on position. Inside a ` ```python ` fenced code block, the language is detected as Python and block expansion applies. Inside ` ```bash `, it's detected as shell and single-line behavior applies.

### Blank lines

On blank lines, both `SendCode` and `SendLine` send a bare Enter (newline) to the terminal and advance the cursor.

## Architecture

```
crates/send_code/src/
  send_code.rs        # Action definitions, init(), dispatch, debug toast
  settings.rs         # SendCodeSettings (target, bracketed paste, chunk sizes, debug)
  code_getter.rs      # Extract code from editor (selection, line, block, file)
  block_expander.rs   # Language-aware block expansion (Python, R, Julia, Markdown)
  senders/
    mod.rs            # Target dispatch
    zed_terminal.rs   # Built-in terminal via TerminalPanel
    ghostty.rs        # AppleScript -> Ghostty
    iterm.rs          # AppleScript -> iTerm
    terminal_app.rs   # AppleScript -> Terminal.app
    cmux.rs           # CLI subprocess -> cmux
    tmux.rs           # CLI subprocess -> tmux
```

## Quick Start

1. Add keybindings to your keymap (see above)
2. Open a file and the terminal panel
3. Start a REPL in the terminal (e.g., `R`, `python`, `julia`)
4. Place your cursor in a code block and press `ctrl-enter`
5. To debug what's being sent, set `"debug": true` in settings and click "Send" on the toast to confirm
