local wezterm = require 'wezterm'
local mux = wezterm.mux
local config = {}

wezterm.on('gui-startup', function(cmd)
  -- allow `wezterm start -- something` to affect what we spawn
  -- in our initial window
  local args = {}
  if cmd then
    args = cmd.args
  end

  -- Set a workspace for coding on a current project
  -- Top pane is for the editor, bottom pane is for the build tool
  local project_dir = "/home/lyj/.cache/github/mdict-cli-rs"
  local tab, build_pane, window = mux.spawn_window {
    workspace = 'coding',
    cwd = project_dir,
    args = args,
  }

  build_pane:send_text 'rm /tmp/my_pipe -f; mkfifo /tmp/my_pipe\n'
  build_pane:send_text './server.sh\n'
  -- build_pane:send_text 'mdict-cli-rs anki\n'


  -- A workspace for interacting with a local machine that
  -- runs some docker containers for home automation
  local tab, pane, window = window:spawn_tab {
    cwd = project_dir,
  }
  pane:send_text './start_carbonyl.sh\n'

  -- We want to startup in the coding workspace
  -- mux.set_active_workspace 'automation'
end)

return config
