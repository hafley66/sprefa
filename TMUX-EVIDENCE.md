# tmux visibility evidence

Evidence-only. Every claim carries a man page quote, a raw command output block,
or a `path:line` citation. Source: `man tmux` (tmux 3.7b) via `col -b`, live
probes on throwaway sockets, and the unpacked `tmux_interface` 0.4.0 crate at
`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/tmux_interface-0.4.0`.

## 1. Control mode notifications

Verbatim from the CONTROL MODE section of the man page (lines 4321-4416 of the
col-extracted text). One row per notification the man page documents.

| notification | arguments | man page sentence |
|---|---|---|
| `%client-detached` | `client` | The client has detached. |
| `%client-session-changed` | `client session-id name` | The client is now attached to the session with ID session-id, which is named name. |
| `%config-error` | `error` | An error has happened in a configuration file. |
| `%continue` | `pane-id` | The pane has been continued after being paused (if the pause-after flag is set, see refresh-client -A). |
| `%exit` | `[reason]` | The tmux client is exiting immediately, either because it is not attached to any session or an error occurred.  If present, reason describes why the client exited. |
| `%extended-output` | `pane-id age ... : value` | New form of %output sent when the pause-after flag is set.  age is the time in milliseconds for which tmux had buffered the output before it was sent.  Any subsequent arguments up until a single ‘:’ are for future use and should be ignored. |
| `%layout-change` | `window-id window-layout window-visible-layout window-flags` | The layout of a window with ID window-id changed.  The new layout is window-layout.  The window's visible layout is window-visible-layout and the window flags are window-flags. |
| `%message` | `message` | A message sent with the display-message command. |
| `%output` | `pane-id value` | A window pane produced output.  value escapes non-printable characters and backslash as octal \xxx. |
| `%pane-mode-changed` | `pane-id` | The pane with ID pane-id has changed mode. |
| `%paste-buffer-changed` | `name` | Paste buffer name has been changed. |
| `%paste-buffer-deleted` | `name` | Paste buffer name has been deleted. |
| `%pause` | `pane-id` | The pane has been paused (if the pause-after flag is set). |
| `%session-changed` | `session-id name` | The client is now attached to the session with ID session-id, which is named name. |
| `%session-renamed` | `name` | The current session was renamed to name. |
| `%session-window-changed` | `session-id window-id` | The session with ID session-id changed its active window to the window with ID window-id. |
| `%sessions-changed` |  | A session was created or destroyed. |
| `%subscription-changed` | `name session-id window-id window-index pane-id ... : value` | The value of the format associated with subscription name has changed to value.  See refresh-client -B.  Any arguments after pane-id up until a single ‘:’ are for future use and should be ignored. |
| `%unlinked-window-add` | `window-id` | The window with ID window-id was created but is not linked to the current session. |
| `%unlinked-window-close` | `window-id` | The window with ID window-id, which is not linked to the current session, was closed. |
| `%unlinked-window-renamed` | `window-id new-name` | The window with ID window-id, which is not linked to the current session, was renamed. |
| `%window-add` | `window-id` | The window with ID window-id was linked to the current session. |
| `%window-close` | `window-id` | The window with ID window-id closed. |
| `%window-pane-changed` | `window-id pane-id` | The active pane in the window with ID window-id changed to the pane with ID pane-id. |
| `%window-renamed` | `window-id name` | The window with ID window-id was renamed to name. |

Control mode entry (`-C` and `-CC`), verbatim from the -C option description:

```
     -C 	   Start in control mode (see the CONTROL MODE section).
		   Given twice (-CC) disables echo.
```

Control mode protocol framing, verbatim from the CONTROL MODE section:

```
     In control mode, a client sends tmux commands or command sequences
     terminated by newlines on standard input.	Each command will produce one
     block of output on standard output.  An output block consists of a %begin
     line followed by the output (which may be empty).	The output block ends
     with a %end or %error.  %begin and matching %end or %error have three
     arguments: an integer time (as seconds from epoch), command number and
     flags (currently not used).  For example:

	   %begin 1363006971 2 1
	   0: ksh* (1 panes) [80x24] [layout b25f,80x24,0,0,2] @2 (active)
	   %end 1363006971 2 1

     The refresh-client -C command may be used to set the size of a client in
     control mode.

     In control mode, tmux outputs notifications.  A notification will never
     occur inside an output block.
```

The `refresh-client -B` subscription mechanism, verbatim from refresh-client:

```
	     -B sets a subscription to a format for a control mode client.
	     The argument is split into three items by colons: name is a name
	     for the subscription; what is a type of item to subscribe to;
	     format is the format.  After a subscription is added, changes to
	     the format are reported with the %subscription-changed
	     notification, at most once a second.  If only the name is given,
	     the subscription is removed.  what may be empty to check the
	     format only for the attached session, or one of: a pane ID such
	     as ‘%0’; ‘%*’ for all panes in the attached session; a window ID
	     such as ‘@0’; or ‘@*’ for all windows in the attached session.
```

## 2. Hooks

The HOOKS section states every CONTROL MODE notification is also a hook
(except `%exit`), then lists additional hooks. Verbatim:

```
     All the notifications listed in the CONTROL MODE section are hooks
     (without any arguments), except %exit.  The following additional hooks
     are available:
```

Every hook name the HOOKS section lists (the additional hooks), with verbatim
description:

| hook | verbatim description |
|---|---|
| `alert-activity` | Run when a window has activity.  See monitor-activity. |
| `alert-bell` | Run when a window has received a bell.  See monitor-bell. |
| `alert-silence` | Run when a window has been silent.  See monitor-silence. |
| `client-active` | Run when a client becomes the latest active client of its session. |
| `client-attached` | Run when a client is attached. |
| `client-detached` | Run when a client is detached |
| `client-focus-in` | Run when focus enters a client |
| `client-focus-out` | Run when focus exits a client |
| `client-resized` | Run when a client is resized. |
| `client-session-changed` | Run when a client's attached session is changed. |
| `client-light-theme` | Run when a client switches to a light theme. |
| `client-dark-theme` | Run when a client switches to a dark theme. |
| `command-error` | Run when a command fails. |
| `pane-died` | Run when the program running in a pane exits, but remain-on-exit is on so the pane has not closed. |
| `pane-exited` | Run when the program running in a pane exits. |
| `pane-focus-in` | Run when the focus enters a pane, if the focus-events option is on. |
| `pane-focus-out` | Run when the focus exits a pane, if the focus-events option is on. |
| `pane-set-clipboard` | Run when the terminal clipboard is set using the xterm(1) escape sequence. |
| `session-created` | Run when a new session created. |
| `session-closed` | Run when a session closed. |
| `session-renamed` | Run when a session is renamed. |
| `window-layout-changed` | Run when a window layout is changed. |
| `window-linked` | Run when a window is linked into a session. |
| `window-renamed` | Run when a window is renamed. |
| `window-resized` | Run when a window is resized.  This may be after the client-resized hook is run. |
| `window-unlinked` | Run when a window is unlinked from a session. |

The HOOKS section also documents the generic `after-` command-hook convention,
verbatim:

```
     A command's after hook is run after it completes, except when the command
     is run as part of a hook itself.  They are named with an ‘after-’ prefix.
```

### Hooks that carry visibility meaning

Selecting only the hooks whose names concern client attach/detach, session
switching, window selection, pane focus, and pane mode (including the CONTROL
MODE notification hooks, which the man page states are hooks).

| hook | verbatim description |
|---|---|
| `%client-detached` (also hook) | The client has detached. |
| `%client-session-changed` (also hook) | The client is now attached to the session with ID session-id, which is named name. |
| `%pane-mode-changed` (also hook) | The pane with ID pane-id has changed mode. |
| `%session-changed` (also hook) | The client is now attached to the session with ID session-id, which is named name. |
| `%session-renamed` (also hook) | The current session was renamed to name. |
| `%session-window-changed` (also hook) | The session with ID session-id changed its active window to the window with ID window-id. |
| `%window-pane-changed` (also hook) | The active pane in the window with ID window-id changed to the pane with ID pane-id. |
| `%window-renamed` (also hook) | The window with ID window-id was renamed to name. |
| `client-active` | Run when a client becomes the latest active client of its session. |
| `client-attached` | Run when a client is attached. |
| `client-detached` | Run when a client is detached |
| `client-focus-in` | Run when focus enters a client |
| `client-focus-out` | Run when focus exits a client |
| `client-resized` | Run when a client is resized. |
| `client-session-changed` | Run when a client's attached session is changed. |
| `pane-focus-in` | Run when the focus enters a pane, if the focus-events option is on. |
| `pane-focus-out` | Run when the focus exits a pane, if the focus-events option is on. |
| `session-created` | Run when a new session created. |
| `session-closed` | Run when a session closed. |
| `session-renamed` | Run when a session is renamed. |
| `window-layout-changed` | Run when a window layout is changed. |
| `window-linked` | Run when a window is linked into a session. |
| `window-renamed` | Run when a window is renamed. |
| `window-resized` | Run when a window is resized.  This may be after the client-resized hook is run. |
| `window-unlinked` | Run when a window is unlinked from a session. |

Pane focus hooks and the `focus-events` option. The man page does say the pane
focus hooks require the option, verbatim: `pane-focus-in  Run when the focus
enters a pane, if the focus-events option is on.`

`set-hook` syntax, verbatim:

```
     set-hook [-agpRuw] [-t target-pane] hook-name [command]
	     Without -R, sets (or with -u unsets) hook hook-name to command.
	     The flags are the same as for set-option.

	     With -R, run hook-name immediately.
```

Because "The flags are the same as for set-option", the meaning of `-a`, `-g`,
`-w`, `-p` comes from `set-option`, quoted verbatim:

```
	     Set a pane option with -p, a window option with -w, a server
	     option with -s, otherwise a session option.  If the option is not
	     a user option, -w or -s may be unnecessary - tmux will infer the
	     scope from the option name, assuming -w for pane options.	If -g
	     is given, the global session or window option is set.

	     With -a, and if the option expects a string or a style, value is
	     appended to the existing setting.
```

`-t` selects the target-pane for the hook (session, window, or pane scope).

What `run-shell` does inside a hook and which format variables expand for it,
verbatim:

```
     run-shell [-bCE] [-c start-directory] [-d delay] [-t target-pane]
	     [shell-command [argument ...]]
		   (alias: run)
	     Execute shell-command using /bin/sh or (with -C) a tmux command
	     in the background without creating a window.  Before being
	     executed, shell-command is expanded using the rules specified in
	     the FORMATS section.  If argument values are given, they are
	     available as ‘#{1}’, ‘#{2}’ and so on.  For example:

		   run-shell 'myscript.sh #1 #2' foo bar
```

Inside a hook, the hook context format variables are available (from the
FORMATS section); verbatim:

```
     hook			     Name of running hook, if any
     hook_client		     Name of client where hook was run, if any
     hook_pane			     ID of pane where hook was run, if any
     hook_session		     ID of session where hook was run, if any
     hook_session_name		     Name of session where hook was run, if any
     hook_window		     ID of window where hook was run, if any
     hook_window_name		     Name of window where hook was run, if any
```

## 3. Format variables

From the FORMATS section. Every variable whose name is exactly one of, or
contains, any of: `client_`, `session_`, `window_`, `pane_`, `active`,
`attached`, `focus`, `visible`, `activity`, `zoomed` (variants `_zoomed`/
`zoomed_flag`), `mode_`, `scroll_`, `alternate_`, `cursor_`, `history_`.
Sorted alphabetically. Verbatim descriptions.

| variable | verbatim man page description |
|---|---|
| active_window_index | Index of active window in session |
| alternate_on | 1 if pane is in alternate screen |
| alternate_saved_x | Saved cursor X in alternate screen |
| alternate_saved_y | Saved cursor Y in alternate screen |
| client_activity | Time client last had activity |
| client_cell_height | Height of each client cell in pixels |
| client_cell_width | Width of each client cell in pixels |
| client_control_mode | 1 if client is in control mode |
| client_created | Time client created |
| client_discarded | Bytes discarded when client behind |
| client_flags | List of client flags |
| client_height | Height of client |
| client_key_table | Current key table |
| client_last_session | Name of the client's last session |
| client_name | Name of client |
| client_pid | PID of client process |
| client_prefix | 1 if prefix key has been pressed |
| client_readonly | 1 if client is read-only |
| client_session | Name of the client's session |
| client_termfeatures | Terminal features of client, if any |
| client_termname | Terminal name of client |
| client_termtype | Terminal type of client, if available |
| client_tty | Pseudo terminal of client |
| client_uid | UID of client process |
| client_user | User of client process |
| client_utf8 | 1 if client supports UTF-8 |
| client_width | Width of client |
| client_written | Bytes written to client |
| copy_cursor_hyperlink | Hyperlink under cursor in copy mode |
| copy_cursor_line | Line the cursor is on in copy mode |
| copy_cursor_word | Word under cursor in copy mode |
| copy_cursor_x | Cursor X position in copy mode |
| copy_cursor_y | Cursor Y position in copy mode |
| cursor_blinking | 1 if the cursor is blinking |
| cursor_character | Character at cursor in pane |
| cursor_colour | Cursor colour in pane |
| cursor_flag | Pane cursor flag |
| cursor_shape | Cursor shape in pane |
| cursor_very_visible | 1 if the cursor is in very visible mode |
| cursor_x | Cursor X position in pane |
| cursor_y | Cursor Y position in pane |
| history_bytes | Number of bytes in window history |
| history_limit | Maximum window history lines |
| history_size | Size of history in lines |
| hook_session_name | Name of session where hook was run, if any |
| hook_window_name | Name of window where hook was run, if any |
| keypad_cursor_flag | Pane keypad cursor flag |
| last_window_index | Index of last window in session |
| next_session_id | Unique session ID for next new session |
| next_window_active | 1 if next window in W: loop is active |
| next_window_index | Index of next window in W: loop |
| pane_active | 1 if active pane |
| pane_at_bottom | 1 if pane is at the bottom of window |
| pane_at_left | 1 if pane is at the left of window |
| pane_at_right | 1 if pane is at the right of window |
| pane_at_top | 1 if pane is at the top of window |
| pane_bg | Pane background colour |
| pane_bottom | Bottom of pane |
| pane_current_command | Current command if available |
| pane_current_path | Current path if available |
| pane_dead | 1 if pane is dead |
| pane_dead_signal | Exit signal of process in dead pane |
| pane_dead_status | Exit status of process in dead pane |
| pane_dead_time | Exit time of process in dead pane |
| pane_fg | Pane foreground colour |
| pane_flags | Pane flags |
| pane_floating_flag | 1 if pane is floating |
| pane_format | 1 if format is for a pane |
| pane_height | Height of pane |
| pane_id | #D	     Unique pane ID |
| pane_in_mode | Number of modes pane is in |
| pane_index | #P	     Index of pane |
| pane_input_off | 1 if input to pane is disabled |
| pane_key_mode | Extended key reporting mode in this pane |
| pane_last | 1 if last pane |
| pane_left | Left of pane |
| pane_marked | 1 if this is the marked pane |
| pane_marked_set | 1 if a marked pane is set |
| pane_mode | Name of pane mode, if any |
| pane_path | Path of pane (can be set by application) |
| pane_pb_progress | Pane progress bar progress percentage (can be set by application) |
| pane_pb_state | Pane progress bar state, one of hidden, normal, error, indeterminate, paused (can be set by application) |
| pane_pid | PID of first process in pane |
| pane_pipe | 1 if pane is being piped |
| pane_pipe_pid | PID of pipe process, if any |
| pane_right | Right of pane |
| pane_search_string | Last search string in copy mode |
| pane_start_command | Command pane started with |
| pane_start_path | Path pane started with |
| pane_synchronized | 1 if pane is synchronized |
| pane_tabs | Pane tab positions |
| pane_title | #T	     Title of pane (can be set by application) |
| pane_top | Top of pane |
| pane_tty | Pseudo terminal of pane |
| pane_unseen_changes | 1 if there were changes in pane while in mode |
| pane_width | Width of pane |
| pane_x | X position of pane |
| pane_y | Y position of pane |
| pane_z | Z position of pane |
| pane_zoomed_flag | 1 if pane is zoomed |
| prev_window_active | 1 if previous window in W: loop is active |
| prev_window_index | Index of previous window in W: loop |
| scroll_position | Scroll position in copy mode |
| scroll_region_lower | Bottom of scroll region in pane |
| scroll_region_upper | Top of scroll region in pane |
| selection_active | 1 if selection started and changes with the cursor in copy mode |
| session_active | 1 if session active |
| session_activity | Time of session last activity |
| session_activity_flag | 1 if any window in session has activity |
| session_alerts | List of window indexes with alerts |
| session_attached | Number of clients session is attached to |
| session_attached_list | List of clients session is attached to |
| session_bell_flag | 1 if any window in session has bell |
| session_created | Time session created |
| session_format | 1 if format is for a session |
| session_group | Name of session group |
| session_group_attached | Number of clients sessions in group are attached to |
| session_group_attached_list | List of clients sessions in group are attached to |
| session_group_list | List of sessions in group |
| session_group_many_attached | 1 if multiple clients attached to sessions in group |
| session_group_size | Size of session group |
| session_grouped | 1 if session in a group |
| session_id | Unique session ID |
| session_last_attached | Time session last attached |
| session_many_attached | 1 if multiple clients attached |
| session_marked | 1 if this session contains the marked pane |
| session_name | #S	     Name of session |
| session_path | Working directory of session |
| session_silence_flag | 1 if any window in session has silence alert |
| session_stack | Window indexes in most recent order |
| session_windows | Number of windows in session |
| window_active | 1 if window active |
| window_active_clients | Number of clients viewing this window |
| window_active_clients_list | List of clients viewing this window |
| window_active_sessions | Number of sessions on which this window is active |
| window_active_sessions_list | List of sessions on which this window is active |
| window_activity | Time of window last activity |
| window_activity_flag | 1 if window has activity |
| window_bell_flag | 1 if window has bell |
| window_bigger | 1 if window is larger than client |
| window_cell_height | Height of each cell in pixels |
| window_cell_width | Width of each cell in pixels |
| window_end_flag | 1 if window has the highest index |
| window_flags | #F	     Window flags with # escaped as ## |
| window_format | 1 if format is for a window |
| window_height | Height of window |
| window_id | Unique window ID |
| window_index | #I	     Index of window |
| window_last_flag | 1 if window is the last used |
| window_layout | Window layout description, ignoring zoomed window panes |
| window_linked | 1 if window is linked across sessions |
| window_linked_sessions | Number of sessions this window is linked to |
| window_linked_sessions_list | List of sessions this window is linked to |
| window_marked_flag | 1 if window contains the marked pane |
| window_name | #W	     Name of window |
| window_offset_x | X offset into window if larger than client |
| window_offset_y | Y offset into window if larger than client |
| window_panes | Number of panes in window |
| window_raw_flags | Window flags with nothing escaped |
| window_silence_flag | 1 if window has silence alert |
| window_stack_index | Index in session most recent stack |
| window_start_flag | 1 if window has the lowest index |
| window_visible_layout | Window layout description, respecting zoomed window panes |
| window_width | Width of window |
| window_zoomed_flag | 1 if window is zoomed |

Total rows: 165.

## 4. Live probes

Socket `SOCK=boop-test-44471`. Every command and its raw output below.

```
$ tmux -L $SOCK new-session -d -s probe1 -x 80 -y 24
(no output)
$ tmux -L $SOCK new-session -d -s probe2 -x 80 -y 24
(no output)
$ tmux -L $SOCK new-window -t probe1
(no output)
$ tmux -L $SOCK list-clients -F 'client=#{client_name} session=#{client_session} tty=#{client_tty} activity=#{client_activity} created=#{client_created} flags=#{client_flags} width=#{client_width} height=#{client_height}'
(no output: no clients attached to the throwaway server)
$ tmux -L $SOCK list-sessions -F 'session=#{session_name} attached=#{session_attached} activity=#{session_activity} windows=#{session_windows} id=#{session_id}'
session=probe1 attached=0 activity=1786292880 windows=2 id=$0
session=probe2 attached=0 activity=1786292880 windows=1 id=$1
$ tmux -L $SOCK list-windows -a -F 'session=#{session_name} window=#{window_index} id=#{window_id} active=#{window_active} activity=#{window_activity} flags=#{window_flags} name=#{window_name}'
session=probe1 window=0 id=@0 active=0 activity=1786292880 flags=- name=bash
session=probe1 window=1 id=@2 active=1 activity=1786292880 flags=* name=tmux
session=probe2 window=0 id=@1 active=1 activity=1786292880 flags=* name=tmux
$ tmux -L $SOCK list-panes -a -F 'session=#{session_name} window=#{window_index} pane=#{pane_index} id=#{pane_id} active=#{pane_active} pid=#{pane_pid} cmd=#{pane_current_command} path=#{pane_current_path} in_mode=#{pane_in_mode} title=#{pane_title} start=#{pane_start_command}'
session=probe1 window=0 pane=0 id=%0 active=1 pid=44475 cmd=bash path=/Users/chrishafley/projects/sprefa-lanes/tmuxvis in_mode=0 title=Chriss-MacBook-Pro.local start=
session=probe1 window=1 pane=0 id=%2 active=1 pid=44481 cmd=bash path=/Users/chrishafley/projects/sprefa-lanes/tmuxvis in_mode=0 title=Chriss-MacBook-Pro.local start=
session=probe2 window=0 pane=0 id=%1 active=1 pid=44477 cmd=bash path=/Users/chrishafley/projects/sprefa-lanes/tmuxvis in_mode=0 title=Chriss-MacBook-Pro.local start=
$ tmux -L $SOCK display-message -p -t probe1 '#{client_activity} #{session_activity} #{window_active} #{pane_active}'
 1786292880 1 1
$ tmux -L $SOCK set -t probe1 @boop-owner human
(no output)
$ tmux -L $SOCK show-options -t probe1 -v @boop-owner
human
$ tmux -L $SOCK setw -t probe1:0 @boop-lane catalog9
(no output)
$ tmux -L $SOCK show-options -w -t probe1:0 -v @boop-lane
catalog9
$ tmux -L $SOCK set -p -t probe1:0.0 @boop-kind agent
(no output)
$ tmux -L $SOCK show-options -p -t probe1:0.0 -v @boop-kind
agent
$ tmux -L $SOCK list-panes -a -F 'pane=#{pane_id} kind=#{@boop-kind} lane=#{@boop-lane} owner=#{@boop-owner}'
pane=%0 kind=agent lane=catalog9 owner=human
pane=%2 kind= lane= owner=human
pane=%1 kind= lane= owner=
```

User options scoped at session (`@boop-owner`), window (`@boop-lane`), and pane
(`@boop-kind`) read back correctly and are visible in `list-panes -F`.

The three `capture-pane` variants. All three returned exactly 24 lines (the full
visible pane); history was empty so the trailing/start range had nothing extra to
return.

```
$ tmux -L $SOCK capture-pane -p -t probe1:0.0 | wc -l
24
$ tmux -L $SOCK capture-pane -p -S -10 -t probe1:0.0 | wc -l
24
$ tmux -L $SOCK capture-pane -p -S - -t probe1:0.0 | wc -l
24
```

Raw capture output (a `-S -10` run, representative of all three; the pane was a
fresh bash with 23 trailing blank lines so all three look identical):

```
 chrishafley  ～  projects  ／ sprefa-lanes  ／ tmuxvis  ／  lane/tmuxvis  1+  3⚑
  $  ～
λ.





















```

`display-message` scroll/mode probe:

```
$ tmux -L $SOCK display-message -p -t probe1:0.0 '#{scroll_position} #{pane_in_mode} #{history_size} #{pane_height}'
 0 0 24
```

That output is 4 fields: `#{scroll_position}` empty (no scroll buffer populated),
`#{pane_in_mode}`=0, `#{history_size}`=0, `#{pane_height}`=24.

The difference between the three capture-pane variants: with no `-S` and with
`-S -10` and with `-S -`, the returned line count is identical (24) because the
pane's scrollback history is empty (`history_size`=0). With `-S -10` the range
only extends 10 lines back into an empty scrollback, and `-S -` extends to the
full history start, so none adds lines over the visible screen.

Raw `pipe-pane` probe:

```
$ tmux -L $SOCK pipe-pane -t probe1:0.0 -o 'cat >> /tmp/boop-pipe-44471.log'
(no output)
$ tmux -L $SOCK send-keys -t probe1:0.0 'echo hello-from-probe' Enter
(no output)
$ sleep 1
$ tmux -L $SOCK pipe-pane -t probe1:0.0
(no output)
$ wc -c /tmp/boop-pipe-44471.log
584 /tmp/boop-pipe-44471.log
$ cat /tmp/boop-pipe-44471.log
echo hello-from-probe
[?2004lhello-from-probe
[?2004h[38;5;250m[48;5;240m chrishafley [48;5;31m[38;5;240m [0m[38;5;15m[48;5;31m ~ [48;5;237m[38;5;31m [0m[38;5;250m[48;5;237m projects [48;5;237m[38;5;244m [0m[38;5;250m[48;5;237m sprefa-lanes [48;5;237m[38;5;244m [0m[38;5;254m[48;5;237m tmuxvis [48;5;161m[38;5;237m [0m[38;5;15m[48;5;161m  lane/tmuxvis [48;5;52m[38;5;161m [0m[38;5;15m[48;5;52m 1+ [48;5;20m[38;5;52m [0m[38;5;15m[48;5;20m 3? [48;5;236m[38;5;20m [0m[38;5;15m[48;5;236m $ [0m[38;5;236m [0m 
λ.
```

`pipe-pane` captures raw pane byte output including the echoed keystroke, the
program response, and the subsequent full prompt redraw with ANSI SGR sequences.

Raw hook probe. The exact `set-hook` command from the brief errors with a
syntax error (the `\"` escaping survives bash single quotes as literal
backslashes), so the hook is not set and `show-hooks -g` lists only the default
hook set. This failure is itself the raw finding.

```
$ tmux -L $SOCK set-hook -g client-attached 'run-shell \"echo attached client=#{client_name} session=#{client_session} at $(date +%s) >> /tmp/boop-hook-44471.log\"'
syntax error
$ tmux -L $SOCK show-hooks -g
after-bind-key
after-capture-pane
after-copy-mode
after-display-message
after-display-panes
after-kill-pane
after-list-buffers
after-list-clients
after-list-keys
after-list-panes
after-list-sessions
after-list-windows
after-load-buffer
after-lock-server
after-new-session
after-new-window
after-paste-buffer
after-pipe-pane
after-queue
after-refresh-client
after-rename-session
after-rename-window
after-resize-pane
after-resize-window
after-save-buffer
after-select-layout
after-select-pane
after-select-window
after-send-keys
after-set-buffer
after-set-environment
after-set-hook
after-set-option
after-show-environment
after-show-messages
after-show-options
after-split-window
after-unbind-key
alert-activity
alert-bell
alert-silence
client-active
client-attached
client-detached
client-focus-in
client-focus-out
client-resized
client-session-changed
client-light-theme
client-dark-theme
command-error
session-closed
session-created
session-renamed
session-window-changed
window-linked
window-unlinked
$ tmux -L $SOCK new-session -d -s probe3
(no output)
$ tmux -L $SOCK send-keys -t probe3 "tmux -L $SOCK attach -t probe1" Enter
(no output)
$ sleep 2
$ cat /tmp/boop-hook-44471.log
cat: /tmp/boop-hook-44471.log: No such file or directory
```

The nested `tmux attach` inside a pane of the same throwaway server creates no
client: tmux blocks nested attach. Raw proof on a fresh socket:

```
$ tmux -L $SOCK capture-pane -p -t probe3:0
tmux -L boop-test-hk3-48822 attach -t base
 chrishafley  ～  projects  ／ sprefa-lanes  ／ tmuxvis  ／  lane/tmuxvis  1+  3?  $  ～
λ. tmux -L boop-test-hk3-48822 attach -t base
sessions should be nested with care, unset $TMUX to force
 chrishafley  ～  projects  ／ sprefa-lanes  ／ tmuxvis  ／  lane/tmuxvis  1+  3?  $
λ.
```

The probe shell is itself inside a tmux session (`TMUX=/private/tmp/tmux-501/default`),
so the tray command returns `sessions should be nested with care, unset $TMUX to
force` and no client is created, so `client-attached` never fires and no hook log
is written.

To preserve the hook-firing evidence, a corrected `set-hook` (single-quoted
command, unescaped inner double quotes) plus a control-mode attach client (a
real client, not a nested pane) fired `client-attached` and wrote the log:

```
$ tmux -L $SOCK set-hook -g client-attached "run-shell 'echo attached client=#{client_name} session=#{client_session} at \$(date +%s) >> /tmp/boop-hk4-49372.log'"
(no output)
$ tmux -L $SOCK show-hooks -g | grep client-attached
client-attached[0] run-shell "echo attached client=#{client_name} session=#{client_session} at $(date +%s) >> /tmp/boop-hk4-49372.log"
$ (tmux -L $SOCK -C attach -t base > /tmp/boop-hk4-49372.cm 2>&1 & echo $! > /tmp/boop-hk4-49372.pid)
$ sleep 2
$ cat /tmp/boop-hk4-49372.log
attached client=client-49379 session=base at 1786293000
```

The `client-attached` hook fired for the control-mode client `client-49379`.

Teardown (raw output):

```
$ tmux -L $SOCK kill-server
(no output)
$ tmux -L $SOCK ls
no server running on /private/tmp/tmux-501/boop-test-44471
```

Live coordinator socket, read-only, at the very end:

```
$ tmux -L lanes ls
no server running on /private/tmp/tmux-501/lanes
```

The throwaway probes wrote nothing to the `lanes` server.

## 5. Control mode transcript

Exact brief command and raw bytes (`SOCK2=boop-test-cm-50540`):

```
$ tmux -L $SOCK2 new-session -d -s cm1
(no output)
$ (tmux -L $SOCK2 -C attach -t cm1 & echo $! > /tmp/boop-cm-50540.pid) > /tmp/boop-cm-50540.out 2>&1
$ sleep 1
$ tmux -L $SOCK2 new-window -t cm1
$ tmux -L $SOCK2 select-window -t cm1:0
$ tmux -L $SOCK2 send-keys -t cm1:0 'echo control-mode-probe' Enter
$ sleep 2
$ kill $(cat /tmp/boop-cm-50540.pid)
$ tmux -L $SOCK2 kill-server
$ cat /tmp/boop-cm-50540.out
%begin 1786293012 283 0
%end 1786293012 283 0
%session-changed $0 cm1
%exit
```

`%notification` lines that actually appeared, one per line:

```
%session-changed
%exit
```

(`%begin`/`%end` are output-block framing, not notifications.)

This negative-leaning result is environment-limited: in this non-interactive
shell the background control client has no open stdin/tty, reads EOF on stdin
after attaching, and exits with `%exit` before the later commands run, so
`new-window`/`select-window`/`send-keys` produce no captured notifications here.

Supplementary probe with stdin held open (`< /dev/zero`) so the control client
persists; the full notification stream for the same command sequence:

```
$ tmux -L $SOCK2 -C attach -t cm1 < /dev/zero > /tmp/boop-cms-55578.out 2>&1 &
$ sleep 1
$ tmux -L $SOCK2 new-window -t cm1
$ tmux -L $SOCK2 select-window -t cm1:0
$ tmux -L $SOCK2 send-keys -t cm1:0 'echo control-mode-probe' Enter
$ sleep 2
$ kill <pid>
$ tmux -L $SOCK2 kill-server
$ cat /tmp/boop-cms-55578.out
%begin 1786293074 283 0
%end 1786293074 283 0
%session-changed $0 cm1
%output %0 [ANSI: initial pane screen redraw]
%session-window-changed $0 @1
%window-add @1
%window-renamed @1 tmux
%session-window-changed $0 @0
%output %0 echo control-mode-probe[CR][NL][ANSI]control-mode-probe[CR][NL]
%output %0 [ANSI: prompt redraw]
%output %1 [ANSI: new window pane screen]
%window-renamed @1 bash
%exit
```

`%output` values above elide the ANSI byte runs for readability; the raw stream
is at `/tmp/boop-cms-55578.out` in the run environment. `%notification` lines
appearing in the held-stdin transcript, one per line:

```
%session-changed
%output
%session-window-changed
%window-add
%window-renamed
%exit
```

## 6. tmux_interface crate

Source: `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/tmux_interface-0.4.0`.

| question | answer |
|---|---|
| does it model control mode at all? | YES `src/control_mode/control_mode.rs`, `src/control_mode/constants.rs`; module parses control-mode output and notifications; 24 `NOTIFICATION_*` constants at `src/control_mode/constants.rs:16-86`. |
| does it model hooks? | YES `src/commands/hooks/show_hooks.rs:88` `cmd.name(SHOW_HOOKS);`, `src/commands/constants.rs:360` `pub const SET_HOOK: &str = "set-hook";`, `:363` `pub const SHOW_HOOKS: &str = "show-hooks";`. |
| does it model `list-clients`? | YES `src/commands/clients_and_sessions/list_clients.rs`, exported as `ListClients, LsC` at `src/commands/clients_and_sessions/mod.rs:119`. |
| does it model `refresh-client -B` subscriptions? | YES, feature-gated. `src/commands/clients_and_sessions/refresh_client.rs:196` `pub fn subscribe(mut self, subscribe: Subscribe<'a>)`, push at `:330` `cmd.push_option(B_UPPERCASE_KEY, arg);` behind `#[cfg(feature = "tmux_3_2")]`; `Subscribe` struct at `src/commands/common/subscribe.rs` is `[-B name:what:format]`. |
| does it parse `%notification` output? | YES `src/control_mode/control_mode.rs:537-545` matches `%subscription-changed` into `Response::SubscriptionChanged`; `%output`, `%layout-change`, `%session-changed`, and all 24 notifications have constants in `src/control_mode/constants.rs`. |
| does it expose format variables as typed structs? | PARTIAL. The `src/variables/{client,pane,session,window}/` structs hold typed fields keyed by short names (`src/variables/pane/pane.rs:26` `pub active: Option<bool>`, `:65` `pub in_mode: Option<bool>`; `src/variables/client/client.rs:58` `pub session: Option<String>`, `:46` `pub name: Option<String>`), not by the literal `#{pane_active}` / `#{client_session}` format-variable identifiers. The strings `pane_active`/`client_session` appear in the crate only inside option-default format strings, e.g. `src/options/window/common/constants.rs:730` `"#{?pane_active,#[reverse],}#{pane_index}#[default] \"#{pane_title}\""`, not as typed field identifiers. So format variables are typed as output struct fields with dropped/shortened prefixes, and the full `#{}` variable names are not exposed as a typed API. |

Snippets (verbatim, with `path:line`):

`src/control_mode/constants.rs:16-86` (notification constants, first and last):

```
16:pub const NOTIFICATION_CLIENT_DETACHED: &str = "%client-detached";
...
86:pub const NOTIFICATION_WINDOW_RENAMED: &str = "%window-renamed";
```

`src/control_mode/control_mode.rs:537-545` (`%subscription-changed` parse):

```
537:            // `%subscription-changed name session-id window-id window-index`
538:            s if s.starts_with(NOTIFICATION_SUBSCRIPTION_CHANGED) => {
...
545:                Ok(Response::SubscriptionChanged {
```

`src/commands/clients_and_sessions/refresh_client.rs:321-333` (`-B` build):

```
321:        // TODO: according to man
322:        // `[-B subscribe]`
323:        #[cfg(feature = "tmux_3_2")]
324:        if let Some(subscribe) = self.subscribe {
325:            let mut arg = format!("%{}", subscribe.name);
...
330:            cmd.push_option(B_UPPERCASE_KEY, arg);
```

`src/commands/common/subscribe.rs` (`Subscribe` struct doc):

```
/// [-B name:what:format]
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct Subscribe<'a> {
    pub name: Cow<'a, str>,
    pub what: Option<usize>,
    pub format: Option<usize>,
}
```

`src/variables/pane/pane.rs:26,65` (typed pane fields):

```
26:    pub active: Option<bool>,
...
65:    pub in_mode: Option<bool>,
```

`src/variables/client/client.rs:46,58` (typed client fields):

```
46:    pub name: Option<String>,
...
58:    pub session: Option<String>,
```

`src/options/window/common/constants.rs:730` (format-variable string, not a typed field):

```
730:    "#{?pane_active,#[reverse],}#{pane_index}#[default] \"#{pane_title}\"";
```
