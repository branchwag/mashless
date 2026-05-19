-- vimprove.report
-- Renders an analysis into a Markdown readout and writes it to disk.

local M = {}

local function plural(n, word)
  return string.format('%d %s%s', n, word, n == 1 and '' or 's')
end

local function fmt_duration(seconds)
  seconds = math.max(0, math.floor(seconds))
  local h = math.floor(seconds / 3600)
  local m = math.floor((seconds % 3600) / 60)
  local s = seconds % 60
  if h > 0 then
    return string.format('%dh %dm %ds', h, m, s)
  elseif m > 0 then
    return string.format('%dm %ds', m, s)
  end
  return string.format('%ds', s)
end

-- Aggregate stats for a list of runs.
local function summarize(runs)
  local longest, total = 0, 0
  for _, r in ipairs(runs) do
    longest = math.max(longest, r.count)
    total = total + r.count
  end
  return longest, total
end

-- Build the ordered list of tips. Each tip is { heading, severity, lines }.
local function build_tips(A)
  local tips = {}

  -- Vertical movement -------------------------------------------------------
  if #A.vertical > 0 then
    local longest, total = summarize(A.vertical)
    local longest_run
    for _, r in ipairs(A.vertical) do
      if r.count == longest then
        longest_run = r
      end
    end
    local lines = {
      string.format(
        'You walked vertically with `j`/`k` in long unbroken runs %s '
          .. '(longest: **%d presses in a row**).',
        plural(#A.vertical, 'time'),
        longest
      ),
    }
    if longest_run and longest_run.dist > 0 then
      lines[#lines + 1] = string.format(
        'That longest run moved you **%d lines** — `%d%s` does it in 2-3 keystrokes.',
        longest_run.dist,
        longest_run.dist,
        (longest_run.tok == 'k' or longest_run.tok == '<Up>') and 'k' or 'j'
      )
    end
    vim.list_extend(lines, {
      'Faster ways to travel vertically:',
      '  - `{count}j` / `{count}k` — turn on `:set relativenumber` and the count is shown for you.',
      '  - `}` / `{` — jump by paragraph / blank-line block.',
      '  - `<C-d>` / `<C-u>` — scroll half a page and keep the cursor centred.',
      '  - `gg` / `G` / `{line}G` — top, bottom, or an absolute line number.',
      '  - `<C-o>` / `<C-i>` — jump back/forward through your jump history.',
    })
    tips[#tips + 1] = { heading = 'Vertical movement', severity = total >= 30 and 3 or 2, lines = lines }
  end

  -- Horizontal movement -----------------------------------------------------
  if #A.horizontal > 0 then
    local longest, total = summarize(A.horizontal)
    local example
    for _, r in ipairs(A.horizontal) do
      if r.char and not example then
        example = r
      end
    end
    local lines = {
      string.format(
        'You inched sideways with `h`/`l` in long runs %s (longest: **%d in a row**).',
        plural(#A.horizontal, 'time'),
        longest
      ),
    }
    if example and example.char then
      lines[#lines + 1] = string.format(
        'One run ended on the character `%s` — `f%s` would have jumped straight there.',
        example.char,
        example.char
      )
    end
    vim.list_extend(lines, {
      'Faster ways to travel within a line:',
      '  - `w` / `b` / `e` — move word by word.',
      '  - `f{char}` / `t{char}` — jump onto / just before the next occurrence of a char (`;` / `,` to repeat).',
      '  - `0` / `^` / `$` — start of line / first non-blank / end of line.',
      '  - `%` — jump to the matching bracket.',
    })
    tips[#tips + 1] = { heading = 'Horizontal movement', severity = total >= 30 and 3 or 2, lines = lines }
  end

  -- Arrow keys --------------------------------------------------------------
  if A.normal_arrows > 0 then
    tips[#tips + 1] = {
      heading = 'Arrow keys in normal mode',
      severity = 2,
      lines = {
        string.format(
          'You used the arrow keys %s in normal mode.',
          plural(A.normal_arrows, 'time')
        ),
        'Stay on the home row: `h` `j` `k` `l` do the same thing without the reach.',
        'If it helps the habit stick, you can even unmap the arrows in normal mode.',
      },
    }
  end

  if A.insert_arrows > 0 then
    tips[#tips + 1] = {
      heading = 'Arrow keys in insert mode',
      severity = 1,
      lines = {
        string.format(
          'You used the arrow keys %s while in insert mode.',
          plural(A.insert_arrows, 'time')
        ),
        'Repositioning is usually cleaner from normal mode: `<Esc>`, move, then re-enter.',
        'For a single quick hop without leaving insert mode, use `<C-o>{motion}`.',
      },
    }
  end

  -- Character deletion ------------------------------------------------------
  if #A.x_runs > 0 then
    local longest = summarize(A.x_runs)
    tips[#tips + 1] = {
      heading = 'Deleting character by character',
      severity = 2,
      lines = {
        string.format(
          'You pressed `x` repeatedly %s (longest: **%d in a row**).',
          plural(#A.x_runs, 'time'),
          longest
        ),
        'Delete in bigger bites:',
        '  - `{count}x` — delete several characters at once.',
        '  - `dw` / `de` — delete to the end of a word.',
        '  - `d$` (or `D`) — delete to the end of the line.',
        '  - `diw` / `daw` — delete the inner word / a word plus its whitespace.',
      },
    }
  end

  if #A.dd_runs > 0 then
    local longest = summarize(A.dd_runs)
    tips[#tips + 1] = {
      heading = 'Deleting lines one at a time',
      severity = 1,
      lines = {
        string.format(
          'You ran `dd` in repeated bursts %s (longest: **%d lines in a row**).',
          plural(#A.dd_runs, 'time'),
          longest
        ),
        '`{count}dd` deletes several lines at once, and `dap` / `dip` delete a whole paragraph.',
        'Or select with `V`, extend with `j`, then `d`.',
      },
    }
  end

  -- Word-motion spam --------------------------------------------------------
  if #A.word_runs > 0 then
    local longest = summarize(A.word_runs)
    tips[#tips + 1] = {
      heading = 'Long word-motion chains',
      severity = 1,
      lines = {
        string.format(
          'You chained `w`/`b`/`e` in long runs %s (longest: **%d in a row**).',
          plural(#A.word_runs, 'time'),
          longest
        ),
        'For a known target, `f{char}` or a `/search` jumps there directly instead of stepping word by word.',
      },
    }
  end

  if #A.undo_runs > 0 then
    local longest = summarize(A.undo_runs)
    tips[#tips + 1] = {
      heading = 'Long undo streaks',
      severity = 1,
      lines = {
        string.format(
          'You tapped `u` in long streaks %s (longest: **%d in a row**).',
          plural(#A.undo_runs, 'time'),
          longest
        ),
        '`{count}u` undoes several steps at once, and `:earlier 1m` / `g-` travel the undo tree by time.',
      },
    }
  end

  return tips
end

local CHEATSHEET = {
  '## Motion cheat-sheet',
  '',
  '| Goal | Keys |',
  '| --- | --- |',
  '| Jump to line N | `{N}G` or `:{N}<CR>` |',
  '| Down/up N lines | `{N}j` / `{N}k` |',
  '| Next/prev word | `w` / `b` (`e` = end of word) |',
  '| To char X on line | `f{X}` / `t{X}`, repeat with `;` / `,` |',
  '| Line ends | `0` start, `^` first non-blank, `$` end |',
  '| Paragraph jump | `}` / `{` |',
  '| Half-page scroll | `<C-d>` / `<C-u>` |',
  '| Search | `/text<CR>`, `n` / `N`, `*` for word under cursor |',
  '| Matching bracket | `%` |',
  '| Delete word / line | `diw`, `daw`, `dd`, `dap` |',
  '| Jump history | `<C-o>` back, `<C-i>` forward |',
}

-- Render an analysis + session into a list of Markdown lines.
function M.render(A, session)
  local duration = os.difftime(os.time(), session.start_time)
  local files = {}
  for f in pairs(session.files) do
    files[#files + 1] = f
  end
  table.sort(files)

  local tips = build_tips(A)

  local out = {}
  local function add(line)
    out[#out + 1] = line or ''
  end

  add('# vimprove session report')
  add('')
  add('*' .. os.date('%A %d %B %Y, %H:%M') .. '*')
  add('')
  add('## Summary')
  add('')
  add(string.format('- **Session length:** %s', fmt_duration(duration)))
  add(string.format('- **Normal/visual keystrokes:** %d', A.total_keys))
  add(string.format('- **Estimated keystrokes wasted:** ~%d', A.wasted))
  add(string.format('- **Efficiency score:** %d / 100', A.efficiency))
  add(string.format('- **Improvement tips:** %d', #tips))
  if #files > 0 then
    add(string.format('- **Files touched:** %d', #files))
    for _, f in ipairs(files) do
      add('  - `' .. f .. '`')
    end
  end
  add('')

  if #tips == 0 then
    add('## No inefficiencies spotted')
    add('')
    if A.total_keys < 20 then
      add('Barely any motions were recorded this session — not enough to judge.')
    else
      add('Clean session — your motions looked efficient. Keep it up.')
    end
    add('')
    vim.list_extend(out, CHEATSHEET)
    return out
  end

  -- Most severe tips first.
  table.sort(tips, function(a, b)
    return a.severity > b.severity
  end)

  add('## What you can improve')
  add('')
  for i, tip in ipairs(tips) do
    local marker = string.rep('!', tip.severity)
    add(string.format('### %d. %s  `%s`', i, tip.heading, marker))
    add('')
    for _, line in ipairs(tip.lines) do
      add(line)
    end
    add('')
  end

  vim.list_extend(out, CHEATSHEET)
  add('')
  add('---')
  add('*Generated by vimprove. `!` = minor, `!!` = worth fixing, `!!!` = a real time sink.*')
  return out
end

-- Analyze + render + write a report file. Returns the path, or nil if skipped.
function M.write(cfg, A, session)
  local dir = cfg.output_dir
  vim.fn.mkdir(dir, 'p')
  local path = dir .. '/' .. os.date('vimprove-%Y-%m-%d-%H%M%S.md')
  local lines = M.render(A, session)
  local ok = pcall(vim.fn.writefile, lines, path)
  if not ok then
    return nil
  end
  return path
end

return M
