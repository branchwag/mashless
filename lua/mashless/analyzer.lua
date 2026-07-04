-- mashless.analyzer
-- Turns a recorded session into structured findings about inefficient motions.

local M = {}

local VERTICAL = { ['j'] = true, ['k'] = true, ['<Down>'] = true, ['<Up>'] = true }
local HORIZONTAL = { ['l'] = true, ['h'] = true, ['<Right>'] = true, ['<Left>'] = true }
local WORDWISE = { ['w'] = true, ['b'] = true, ['e'] = true, ['W'] = true, ['B'] = true, ['E'] = true }

-- Group the key stream into runs of consecutive identical tokens.
local function find_runs(keylog)
  local runs = {}
  local i, n = 1, #keylog
  while i <= n do
    local tok = keylog[i].tok
    local j = i
    while j < n and keylog[j + 1].tok == tok do
      j = j + 1
    end
    runs[#runs + 1] = { tok = tok, count = j - i + 1, s = i, e = j }
    i = j + 1
  end
  return runs
end

-- Analyze a recorder session. Returns a plain data table the report renders.
function M.analyze(session, cfg)
  local kl = session.keylog
  local runs = find_runs(kl)

  local A = {
    vertical = {}, -- { tok, count, dist, arrow }
    horizontal = {}, -- { tok, count, dist, char, arrow }
    x_runs = {}, -- { count }
    dd_runs = {}, -- { count }  (count = number of dd's)
    word_runs = {}, -- { tok, count }
    undo_runs = {}, -- { count }
    normal_arrows = 0, -- arrow keys used in normal mode
    insert_arrows = session.insert_arrows or 0,
    total_keys = session.total_keys or 0,
    wasted = 0, -- estimated keystrokes that an optimal motion would have saved
  }

  -- Cursor position immediately after a run finishes.
  local function end_pos(run)
    local nxt = kl[run.e + 1]
    if nxt then
      return nxt.line, nxt.col
    end
    return session.last_pos[1], session.last_pos[2]
  end

  for _, r in ipairs(runs) do
    local tok = r.tok

    if VERTICAL[tok] then
      local is_arrow = (tok == '<Up>' or tok == '<Down>')
      if is_arrow then
        A.normal_arrows = A.normal_arrows + r.count
      end
      if r.count >= cfg.vmin then
        local el = end_pos(r)
        local dist = math.abs(el - kl[r.s].line)
        A.vertical[#A.vertical + 1] = { tok = tok, count = r.count, dist = dist, arrow = is_arrow }
        -- An optimal jump costs ~3 keys ({count}{motion}); the rest is waste.
        A.wasted = A.wasted + math.max(0, r.count - 3)
      end
    elseif HORIZONTAL[tok] then
      local is_arrow = (tok == '<Left>' or tok == '<Right>')
      if is_arrow then
        A.normal_arrows = A.normal_arrows + r.count
      end
      if r.count >= cfg.hmin then
        local el, ec = end_pos(r)
        local dist = math.abs(ec - kl[r.s].col)
        local char
        local text = kl[r.e].text
        if text then
          char = text:sub(ec + 1, ec + 1)
          if char == '' or char == ' ' then
            char = nil -- nothing useful to target with f/t
          end
        end
        A.horizontal[#A.horizontal + 1] =
          { tok = tok, count = r.count, dist = dist, char = char, arrow = is_arrow }
        A.wasted = A.wasted + math.max(0, r.count - 2)
      end
    elseif tok == 'x' then
      if r.count >= cfg.xmin then
        A.x_runs[#A.x_runs + 1] = { count = r.count }
        A.wasted = A.wasted + math.max(0, r.count - 2)
      end
    elseif tok == 'd' then
      -- A literal `dd` is two consecutive `d` tokens; `dw`/`dj` leave a lone `d`.
      local dds = math.floor(r.count / 2)
      if dds >= 2 then
        A.dd_runs[#A.dd_runs + 1] = { count = dds }
        A.wasted = A.wasted + math.max(0, dds - 1)
      end
    elseif WORDWISE[tok] then
      if r.count >= 5 then
        A.word_runs[#A.word_runs + 1] = { tok = tok, count = r.count }
        A.wasted = A.wasted + math.max(0, r.count - 3)
      end
    elseif tok == 'u' then
      if r.count >= 4 then
        A.undo_runs[#A.undo_runs + 1] = { count = r.count }
      end
    end
  end

  local denom = math.max(A.total_keys, 1)
  A.efficiency = math.floor((1 - A.wasted / denom) * 100 + 0.5)
  if A.efficiency < 0 then
    A.efficiency = 0
  elseif A.efficiency > 100 then
    A.efficiency = 100
  end

  return A
end

return M
