-- mashless.recorder
-- Captures normal/visual-mode keystrokes (with cursor positions) for the
-- current Neovim session. Insert-mode keys are ignored except arrow keys.

local M = {}

-- Arrow keys, used both to flag normal-mode arrow use and insert-mode arrows.
local ARROWS = {
  ['<Up>'] = true,
  ['<Down>'] = true,
  ['<Left>'] = true,
  ['<Right>'] = true,
}

-- Tokens whose surrounding line text we keep, so the analyzer can suggest
-- f/t/search targets for horizontal motions.
local KEEP_TEXT = {
  ['l'] = true,
  ['h'] = true,
  ['<Right>'] = true,
  ['<Left>'] = true,
  ['w'] = true,
  ['b'] = true,
  ['e'] = true,
  ['W'] = true,
  ['B'] = true,
  ['E'] = true,
}

local ns = vim.api.nvim_create_namespace('mashless_on_key')

local function now_ms()
  return (vim.uv or vim.loop).now()
end

-- Reduce Neovim's mode string to a coarse bucket.
local function classify(mode)
  local c = mode:sub(1, 1)
  if c == 'n' then
    return 'normal'
  elseif c == 'v' or c == 'V' or c == '\22' or c == 's' or c == 'S' or c == '\19' then
    return 'visual'
  elseif c == 'i' then
    return 'insert'
  end
  return 'other'
end

local function new_session()
  return {
    start_time = os.time(),
    start_ms = now_ms(),
    keylog = {}, -- ordered list of { tok, mode, line, col, t, text? }
    total_keys = 0, -- normal + visual keystrokes
    insert_keys = 0,
    insert_arrows = 0,
    files = {}, -- set: absolute path -> true
    last_pos = { 1, 0 }, -- most recent cursor position, kept fresh by CursorMoved
  }
end

M.session = nil

-- Begin recording. Safe to call once per session.
function M.start()
  M.session = new_session()
  local s = M.session

  local first = vim.fn.expand('%:p')
  if first ~= '' then
    s.files[first] = true
  end

  vim.on_key(function(key, typed)
    -- Recording must never break the editor: swallow every error.
    pcall(function()
      local k = typed
      if k == nil or k == '' then
        k = key
      end
      if k == nil or k == '' then
        return
      end

      local tok = vim.fn.keytrans(k)
      local kind = classify(vim.api.nvim_get_mode().mode)

      if kind == 'insert' then
        s.insert_keys = s.insert_keys + 1
        if ARROWS[tok] then
          s.insert_arrows = s.insert_arrows + 1
        end
        return
      end

      if kind ~= 'normal' and kind ~= 'visual' then
        return -- cmdline, terminal, etc.
      end

      s.total_keys = s.total_keys + 1

      -- on_key fires *before* the key is processed, so this is the cursor
      -- position the motion starts from.
      local pos = { 1, 0 }
      local ok, p = pcall(vim.api.nvim_win_get_cursor, 0)
      if ok then
        pos = p
      end

      local entry = {
        tok = tok,
        mode = kind,
        line = pos[1],
        col = pos[2],
        t = now_ms(),
      }
      if KEEP_TEXT[tok] then
        local okl, ln = pcall(vim.api.nvim_get_current_line)
        if okl then
          entry.text = ln
        end
      end
      table.insert(s.keylog, entry)
    end)
  end, ns)
end

-- Stop recording (detach the on_key callback).
function M.stop()
  vim.on_key(nil, ns)
end

return M
