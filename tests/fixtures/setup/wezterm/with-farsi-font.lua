-- Hand-written for farcli's tests: the user has added the Farsi font.
local wezterm = require 'wezterm'
local config = wezterm.config_builder()

config.font = wezterm.font_with_fallback {
  'Cascadia Code',
  { family = "Vazir Code WOL", scale = 1.17 },
}

return config
