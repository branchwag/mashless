# vimprove

A Neovim plugin that watches the Vim motions you actually use, and when you
quit Neovim it writes a Markdown readout of what you could have done more
efficiently — like reaching for `f{char}` instead of mashing `l`, or `12G`
instead of holding `j`.

## What it does

- Records every normal- and visual-mode keystroke for the session (insert-mode
  text is ignored; insert-mode arrow keys are noticed).
- Tracks cursor positions, so suggestions are concrete: *"that run moved you 9
  lines — `9j` does it in 3 keystrokes"*.
- On `:q`, analyzes the session and writes a timestamped report.

### What it flags

| Pattern | Suggestion |
| --- | --- |
| Long `j`/`k` runs | `{count}j`, `}`/`{`, `<C-d>`/`<C-u>`, `{n}G` |
| Long `h`/`l` runs | `w`/`b`/`e`, `f{char}`/`t{char}`, `0`/`^`/`$` |
| Arrow keys (normal mode) | `h` `j` `k` `l` |
| Arrow keys (insert mode) | `<Esc>` + motion, or `<C-o>{motion}` |
| Repeated `x` | `{count}x`, `dw`, `D`, `diw`/`daw` |
| Repeated `dd` | `{count}dd`, `dap`/`dip`, visual `V` + `d` |
| Long `w`/`b`/`e` chains | `f{char}`, `/search` |
| Long `u` streaks | `{count}u`, `:earlier`, `g-` |

## Reports

Written to `stdpath('data')/vimprove/` — on Linux that is
`~/.local/share/nvim/vimprove/vimprove-YYYY-MM-DD-HHMMSS.md`.

Each report has a summary (session length, keystrokes, an efficiency score, an
estimate of wasted keystrokes), the ranked tips, and a motion cheat-sheet.

## Commands

- `:Vimprove` — open the latest report in a new tab.
- `:VimproveReport` — generate and open a report now, without quitting.

## Install

vimprove runs straight from a local clone — there is nothing to publish and
nothing to download from a registry. Clone the repo wherever you like, then
point your plugin manager at that directory.

**1. Clone it somewhere.** Any path works; pick one and remember it:

```sh
git clone <repo-url> ~/vimprove
```

**2. Tell your plugin manager to load it from that directory.**

### lazy.nvim

```lua
{
  dir = vim.fn.expand('~/vimprove'), -- the path you cloned into
  name = 'vimprove',
  lazy = false,                      -- load at startup so motions are recorded from the first key
  config = function()
    require('vimprove').setup()
  end,
}
```

The `dir` key tells lazy.nvim to use a local directory instead of cloning from
a remote. Just make sure the path matches step 1.

### packer.nvim

```lua
use {
  '~/vimprove', -- the path you cloned into
  config = function() require('vimprove').setup() end,
}
```

### Plain `:set runtimepath` (no plugin manager)

```lua
vim.opt.runtimepath:append(vim.fn.expand('~/vimprove'))
require('vimprove').setup()
```

## Configuration

```lua
require('vimprove').setup({
  enabled = true,
  output_dir = vim.fn.stdpath('data') .. '/vimprove',
  min_keys = 20,   -- skip the report for trivially short sessions
  vmin = 3,        -- consecutive j/k before it counts as a streak
  hmin = 5,        -- consecutive h/l before it counts as a streak
  xmin = 3,        -- consecutive x before it counts as a streak
  notify_on_enter = true,
})
```

## How it works

`vim.on_key()` receives every keystroke. Each normal/visual key is stored with
the cursor position it started from. The analyzer groups the stream into runs
of identical keys; a long run of a one-step motion is the core signal for
"this could have been a single counted jump or a smarter motion". The report
module renders the findings as Markdown.

Recording is wrapped in `pcall` throughout — a bug in vimprove can never break
your editing.
