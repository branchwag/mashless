-- mashless — thin Lua shim.
--
-- The plugin's brain lives in a Rust binary (see the crate at the repo root).
-- This shim exists only for the parts that *must* run inside Neovim's Lua
-- runtime: `vim.on_key` (which has no RPC binding), the autocmds, and the user
-- commands. Every event is forwarded over msgpack-RPC to the Rust process,
-- which owns the session, analyzes it, writes the HTML report and opens it in
-- the browser.

local M = {}

M.config = {
  enabled = true,
  -- Where session reports are written.
  output_dir = vim.fn.stdpath('data') .. '/mashless',
  -- Skip writing a report when fewer than this many keystrokes were recorded.
  min_keys = 20,
  -- Run-length thresholds before a streak counts as inefficient.
  vmin = 3, -- consecutive j/k
  hmin = 5, -- consecutive h/l
  xmin = 3, -- consecutive x
  -- On startup, point the user at their previous session's report.
  notify_on_enter = true,
}

-- Channel to the Rust process, and the on_key namespace.
local chan = nil
local ns = vim.api.nvim_create_namespace('mashless_on_key')

-- Tokens whose surrounding line text the analyzer wants, so we only pay for
-- fetching/sending the current line when it's actually useful. Mirrors the
-- Rust `keep_text` set.
local KEEP_TEXT = {
  ['l'] = true, ['h'] = true, ['<Right>'] = true, ['<Left>'] = true,
  ['w'] = true, ['b'] = true, ['e'] = true,
  ['W'] = true, ['B'] = true, ['E'] = true,
}

-- Absolute path to the compiled Rust binary, relative to this file:
--   <root>/lua/mashless/init.lua  ->  <root>/target/release/mashless
local function binary_path()
  local src = debug.getinfo(1, 'S').source:sub(2)
  local root = vim.fn.fnamemodify(src, ':h:h:h')
  local exe = root .. '/target/release/mashless'
  if vim.fn.has('win32') == 1 then
    exe = exe .. '.exe'
  end
  return exe
end

-- Open the newest report in the browser (Rust picks the file and launches it).
local function open_latest()
  if not chan then
    return
  end
  local ok, path = pcall(vim.rpcrequest, chan, 'open_latest')
  if not ok or path == nil or path == '' then
    vim.notify('mashless: no reports yet — try :MashlessReport', vim.log.levels.WARN)
  end
end

function M.setup(opts)
  M.config = vim.tbl_deep_extend('force', M.config, opts or {})
  if not M.config.enabled then
    return
  end

  local exe = binary_path()
  if vim.fn.executable(exe) == 0 then
    vim.notify(
      'mashless: Rust binary not found — run `cargo build --release` in ' ..
        vim.fn.fnamemodify(exe, ':h:h:h'),
      vim.log.levels.ERROR
    )
    return
  end

  chan = vim.fn.jobstart({ exe }, { rpc = true })
  if not chan or chan <= 0 then
    vim.notify('mashless: failed to start the Rust process', vim.log.levels.ERROR)
    chan = nil
    return
  end

  -- Hand the Rust side its configuration; this also (re)starts the session.
  vim.rpcnotify(chan, 'setup', M.config.output_dir, M.config.min_keys,
    M.config.vmin, M.config.hmin, M.config.xmin)

  -- Seed the file the session opened with.
  local first = vim.fn.expand('%:p')
  if first ~= '' then
    vim.rpcnotify(chan, 'buf', first)
  end

  -- The core capture: forward every keystroke (with mode + cursor) to Rust.
  -- on_key fires *before* the key is processed, so this is the position the
  -- motion starts from. Wrapped in pcall — recording must never break editing.
  vim.on_key(function(_key, typed)
    pcall(function()
      -- Only count keys the user actually typed. An empty `typed` means the
      -- key was generated internally — a mapping's RHS, or Neovim expanding a
      -- command (e.g. `x` re-feeds `dl`). Recording those would double-count
      -- and shatter runs like `xxx`, so we drop them.
      if typed == nil or typed == '' then
        return
      end

      local tok = vim.fn.keytrans(typed)
      local mode = vim.api.nvim_get_mode().mode

      local line, col = 1, 0
      local ok, p = pcall(vim.api.nvim_win_get_cursor, 0)
      if ok then
        line, col = p[1], p[2]
      end

      local text = ''
      if KEEP_TEXT[tok] then
        local okl, ln = pcall(vim.api.nvim_get_current_line)
        if okl then
          text = ln
        end
      end

      vim.rpcnotify(chan, 'key', tok, mode, line, col, text)
    end)
  end, ns)

  local grp = vim.api.nvim_create_augroup('Mashless', { clear = true })

  -- Keep the latest cursor position fresh so the final run has an accurate end.
  vim.api.nvim_create_autocmd('CursorMoved', {
    group = grp,
    callback = function()
      local ok, p = pcall(vim.api.nvim_win_get_cursor, 0)
      if ok and chan then
        vim.rpcnotify(chan, 'cursor', p[1], p[2])
      end
    end,
  })

  -- Track which files were visited this session.
  vim.api.nvim_create_autocmd({ 'BufReadPost', 'BufNewFile' }, {
    group = grp,
    callback = function(ev)
      local f = vim.api.nvim_buf_get_name(ev.buf)
      if f ~= '' and chan then
        vim.rpcnotify(chan, 'buf', f)
      end
    end,
  })

  -- The core feature: on quit, ask Rust to write the report and open it in the
  -- browser. rpcrequest blocks so the report is written before Neovim exits.
  vim.api.nvim_create_autocmd('VimLeavePre', {
    group = grp,
    callback = function()
      if chan then
        pcall(vim.rpcrequest, chan, 'report', 'exit')
      end
    end,
  })

  -- Let the user know last session's report is waiting.
  if M.config.notify_on_enter then
    vim.api.nvim_create_autocmd('VimEnter', {
      group = grp,
      callback = function()
        vim.schedule(function()
          local reports = vim.fn.glob(M.config.output_dir .. '/mashless-*.html', false, true)
          if #reports > 0 then
            vim.notify('mashless: last session report ready — :Mashless to view', vim.log.levels.INFO)
          end
        end)
      end,
    })
  end

  -- :Mashless        open the newest report in the browser
  vim.api.nvim_create_user_command('Mashless', open_latest,
    { desc = 'Open the latest mashless report in the browser' })

  -- :MashlessReport  generate a report now, mid-session, and open it
  vim.api.nvim_create_user_command('MashlessReport', function()
    if not chan then
      return
    end
    local ok, path = pcall(vim.rpcrequest, chan, 'report', 'manual')
    if not ok or path == nil or path == '' then
      vim.notify('mashless: nothing recorded yet', vim.log.levels.WARN)
    end
  end, { desc = 'Generate a mashless report now and open it in the browser' })
end

return M
