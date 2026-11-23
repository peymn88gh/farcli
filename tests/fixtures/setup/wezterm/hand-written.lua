-- Hand-written for farcli's tests: a WezTerm configuration with the user's
-- own key bindings, and a font chosen in a way a program cannot follow.
local wezterm = require 'wezterm'
local act = wezterm.action
local config = wezterm.config_builder()

local function font_for(host)
  if host == 'laptop' then
    return wezterm.font 'Cascadia Code'
  end
  return wezterm.font('Iosevka', { weight = 'Medium' })
end

config.font = font_for(wezterm.hostname())
config.font_size = 12.0
config.leader = { key = 'b', mods = 'CTRL', timeout_milliseconds = 800 }
config.keys = {
  { key = '%', mods = 'LEADER|SHIFT', action = act.SplitHorizontal { domain = 'CurrentPaneDomain' } },
  { key = 'x', mods = 'LEADER', action = act.CloseCurrentPane { confirm = true } },
}

return config
