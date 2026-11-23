-- Written by farcli setup. WezTerm had no configuration file before.
local wezterm = require 'wezterm'
local config = wezterm.config_builder()

-- English stays in JetBrains Mono, the font WezTerm uses when none is set.
-- Farsi comes from Vazir Code WOL, made larger so its letters fill a cell.
config.font = wezterm.font_with_fallback {
  'JetBrains Mono',
  { family = 'Vazir Code WOL', scale = 1.2 },
}

return config
