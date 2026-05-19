-- vimprove
-- Records your Vim motions during a session and, when you quit Neovim,
-- writes a Markdown readout of what you could have done more efficiently.

local recorder = require('vimprove.recorder')
local analyzer = require('vimprove.analyzer')
local report = require('vimprove.report')

local M = {}

M.config = {
  enabled = true,
  -- Where session reports are written.
  output_dir = vim.fn.stdpath('data') .. '/vimprove',
  -- Skip writing a report when fewer than this many keystrokes were recorded.
  min_keys = 20,
  -- Run-length thresholds before a streak counts as inefficient.
  vmin = 3, -- consecutive j/k
  hmin = 5, -- consecutive h/l
  xmin = 3, -- consecutive x
  -- On startup, point the user at their previous session's report.
  notify_on_enter = true,
}

M.last_report = nil

-- Analyze the current session and write a report.
-- `reason` is 'exit' (honours min_keys) or 'manual' (always writes).
local function generate(reason)
  local session = recorder.session
  if not session then
    return nil
  end
  if reason == 'exit' and session.total_keys < M.config.min_keys then
    return nil
  end
  local analysis = analyzer.analyze(session, M.config)
  local path = report.write(M.config, analysis, session)
  if path then
    M.last_report = path
  end
  return path
end

-- Path of the newest report on disk, or nil.
local function latest_report()
  if M.last_report and vim.fn.filereadable(M.last_report) == 1 then
    return M.last_report
  end
  local files = vim.fn.glob(M.config.output_dir .. '/vimprove-*.md', false, true)
  if #files == 0 then
    return nil
  end
  table.sort(files)
  return files[#files]
end

-- Open a report in a new tab.
local function open_report(path)
  vim.cmd('tabnew ' .. vim.fn.fnameescape(path))
  vim.bo.filetype = 'markdown'
end

function M.setup(opts)
  M.config = vim.tbl_deep_extend('force', M.config, opts or {})
  if not M.config.enabled then
    return
  end

  recorder.start()

  local grp = vim.api.nvim_create_augroup('Vimprove', { clear = true })

  -- Keep the latest cursor position fresh so the final motion run has an
  -- accurate end point.
  vim.api.nvim_create_autocmd('CursorMoved', {
    group = grp,
    callback = function()
      if recorder.session then
        local ok, p = pcall(vim.api.nvim_win_get_cursor, 0)
        if ok then
          recorder.session.last_pos = p
        end
      end
    end,
  })

  -- Track which files were visited this session.
  vim.api.nvim_create_autocmd({ 'BufReadPost', 'BufNewFile' }, {
    group = grp,
    callback = function(ev)
      if recorder.session then
        local f = vim.api.nvim_buf_get_name(ev.buf)
        if f ~= '' then
          recorder.session.files[f] = true
        end
      end
    end,
  })

  -- The core feature: write the readout when Neovim is closing.
  vim.api.nvim_create_autocmd('VimLeavePre', {
    group = grp,
    callback = function()
      generate('exit')
    end,
  })

  -- Let the user know last session's report is waiting.
  if M.config.notify_on_enter then
    vim.api.nvim_create_autocmd('VimEnter', {
      group = grp,
      callback = function()
        vim.schedule(function()
          if latest_report() then
            vim.notify('vimprove: last session report ready — :Vimprove to view', vim.log.levels.INFO)
          end
        end)
      end,
    })
  end

  -- :Vimprove        open the newest report
  vim.api.nvim_create_user_command('Vimprove', function()
    local path = latest_report()
    if path then
      open_report(path)
    else
      vim.notify('vimprove: no reports yet — try :VimproveReport', vim.log.levels.WARN)
    end
  end, { desc = 'Open the latest vimprove report' })

  -- :VimproveReport  generate a report now, mid-session, and open it
  vim.api.nvim_create_user_command('VimproveReport', function()
    local path = generate('manual')
    if path then
      open_report(path)
    else
      vim.notify('vimprove: nothing recorded yet', vim.log.levels.WARN)
    end
  end, { desc = 'Generate a vimprove report now' })
end

return M
