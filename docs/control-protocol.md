# Control protocol

SoundCraft can be driven completely by other programs: scripts, test harnesses and AI agents.
Every menu item, button, mouse gesture and keystroke maps to a **command** (or to synthetic
input), and every command is reachable from:

| Surface | How |
|---|---|
| Desktop app control channel | `soundcraft --control <port>` (`0` = pick a free port; printed as `SOUNDCRAFT_CONTROL_PORT=…`) |
| CLI against a running app | `soundcraft-cli app --port <port> <method or command id> [JSON]` |
| Headless CLI | `soundcraft-cli run --demo --cmd 'mix.volume={"track":"Kick","db":-6}' --bounce out.wav` |
| MCP (headless) | `soundcraft-cli mcp [--demo]` |
| MCP (bridged to the app) | `soundcraft-cli mcp --connect <port>` |

## Isolated automation profiles

Set `SOUNDCRAFT_CONFIG_DIR` to a fresh directory to isolate UI preferences, `Autosave`, `Presets`, and `logs`.
The nonempty value is used as the complete settings directory, even in portable mode (`portable.txt` beside the executable, which otherwise uses `SoundCraftData` next to it). Unset retains the normal platform paths;
an explicit empty value disables the settings directory instead of falling back to a personal profile. Save synthetic sessions to explicit scratch paths. Do not use `SOUNDCRAFT_NO_PREFS`
when testing preference persistence across restarts.

Run UI acceptance with `--no-audio`. This disables the output player; it does not disable recording
input opened by arm/monitor commands. An automated UI smoke run must never arm a track, enable input
monitoring, or start recording. Control input injects egui events; it does not establish operating-system
input delivery or screen-reader acceptance. Native screenshots use the presented viewport frame.

## Wire format

Newline-delimited JSON over TCP on `127.0.0.1`. One request per line, one reply per line:

```json
{"id": 1, "method": "engine.execute", "params": {"command": "track.new", "params": {"count": 2, "format": "Stereo"}}}
{"id": 1, "ok": true, "result": {"tracks": [12, 13]}}
```

Errors: `{"id": 1, "ok": false, "error": "bad parameters for `mix.volume`: no track named `Nope`"}`.

## Methods

| Method | Params | What it does |
|---|---|---|
| `engine.execute` (alias `command`) | `{command, params}` | Run a command programmatically. **Never opens a dialog**; empty params use defaults or the current selection. |
| `engine.commands` | `{filter?}` | Every command: id, label, menu path, shortcut, params doc, enabled + reason. Includes UI-layer commands (`window.*`). |
| `engine.parity` | `{}` | Parity report against the incumbent's menu catalog. |
| `session.inspect` | `{detail?: "summary"|"full"}` | The document: tracks, clips (with times), inserts (+params when full), sends, routing, automation, markers, groups, busses, sources (`loaded` true when decoded audio is in the pool), selection, zoom (`samples_per_px`, `scroll`), transport, undo label. |
| `ui.inspect` | `{}` | UI state (`window`, panels, dialogs), window size, playing/position, audio device, `edit_layout` (timeline and track row rects), and `midi_layout` (piano roll/velocity rects and MIDI viewport, for clicking). |
| `ui.set` | any `UiState` fields | e.g. `{"window": "Mix", "narrow_mix": true}`. |
| `ui.menu.list` | `{}` | Every catalog menu path and the command that implements it (or null). |
| `ui.menu.invoke` | `{path}` or `{id}` | Like clicking the menu item: may open its dialog (e.g. `"Track > New..."`). |
| `ui.click` | `{x, y, button?, count?, cmd?, shift?, alt?, ctrl?}` | Synthetic click (window points). The reply is sent only after a UI pass has applied the press/release. |
| `ui.drag` | `{x, y, to_x, to_y, steps?}` | Synthetic drag with the primary button. |
| `ui.scroll` | `{x, y, dx?, dy?, unit?: "point"\|"line", steps?, phase?, cmd?, shift?, alt?, ctrl?}` | A two-finger trackpad swipe over `(x, y)`: `dx`/`dy` points spread over `steps` frames (default 8). Positive values move the content right/down (natural scrolling). `unit: "line"` sends mouse-wheel notches instead. With `cmd` it zooms; with `shift` it scrolls sideways. Point scrolling may rubber-band at an edge in egui 0.37; use line units for an exact final offset. An explicit `phase` (`start`, `move`, `end`, `cancel`) sends one unscaled event, including momentum. |
| `ui.zoom` | `{x, y, factor, steps?}` | A trackpad pinch around `(x, y)`: `factor` > 1 spreads the fingers (zooms in), within 0.01–100. |
| `ui.move` | `{x, y}` | Move the pointer. |
| `ui.key` | `{key, cmd?, shift?, alt?, ctrl?}` | Key press (`Space`, `Enter`, `A`, `F7`, `=`…). |
| `ui.text` | `{text}` | Type text into the focused field. At most 64 KiB of UTF-8 text per request. |
| `ui.screenshot` | `{path?}` | PNG of the window (base64 in `png_base64` when no path). Exactly one reply per request. A request is bound to the window geometry, focus state and open session it was admitted for. A geometry or DPI change is retried within its 8 s deadline, then fails `screenshot stale`; a change of focused window or of the open document cancels it (`screenshot canceled: focus changed` / `... the session changed`) because those pixels must not be reported as a capture of the new state. It can also fail `screenshot queue full` (8 pending, 4 in flight), `screenshot too large` (64 MiB per image, 192 MiB aggregate **encoding peak**), `screenshot path longer than 4096 bytes`, or `screenshot unavailable: the window has no capturable size yet` (a first request before any window geometry is known, e.g. minimised before the first paint). Each request reserves its own worst-case encode peak — the captured RGBA image plus the raw copy, PNG buffer and base64 text built during encoding — so several small captures cannot retain or encode full-size images past the aggregate. The callback never writes a file: encoding and writing happen after collection, and a result arriving after the deadline returns `screenshot timed out` without creating the file. A native hidden/minimised window is asked for an explicit paint; browser hidden tabs cannot paint on request and time out. |
| `app.quit` | `{}` | Quit. |

Scripted input (`ui.click`, `ui.drag`, `ui.scroll`, `ui.zoom`, `ui.key`, `ui.text`, `ui.move`) queues at most 4096 waiting events; a request that would exceed that bound fails with `synthetic input queue full` instead of growing the queue. A gesture's reply is delivered only after a UI pass has applied all of its events. Requests that follow it on the same connection (inspect, engine commands, screenshots) are therefore ordered after the gesture; requests already queued behind an undelivered gesture wait as well. If no UI pass can deliver the gesture within 15 s, it is canceled with `synthetic input was not delivered` and the request chain continues. Scripted input is automation, not a clipboard action: copy/selection commands it triggers are not published to the OS clipboard or PRIMARY selection.

## Positions and references

### Edit and Mix windows

`window.mix` and `window.edit` open or bring the requested main window to the front.
Both views stay open and share the same session, selection, undo history, transport and audio
engine. `window.toggle_mix_edit` (Cmd+=, Ctrl+= off macOS) switches focus between them.
`window.close` closes the active main view when both are open, keeping the other view and
session alive; the closed view can be reopened from the Window menu.

`window.arrange_tile`, `window.arrange_tile_h`, `window.arrange_tile_v`, and
`window.arrange_cascade` open both views and arrange them within the original main-window
workspace. Horizontal tiles stack the views; vertical tiles place them side by side;
automatic Tile chooses based on the workspace dimensions. Native hosts use separate OS
windows; browser and offscreen hosts use movable, resizable windows within the canvas.

`ui.inspect.main_windows` reports the primary and active view, whether Edit/Mix are open,
and their last known rectangles. Native rectangles use desktop coordinates in egui points;
embedded rectangles use canvas coordinates. `ui.window` identifies the active view and
`ui.both_windows` records whether both are open. Existing preferences and `ui.set` requests
that select a single view remain supported.

Native views repaint independently. Focus-only shortcuts retain their latest destination
without rebuilding the windows, and control/menu requests wake root logic explicitly even
after the Mixer rendered last. Fresh native focus keys are consumed before rendering so an
occluded root cannot delay or replay them.

On macOS, a view fully covered by the other main view is temporarily hidden to
avoid Metal drawable waits during focus transitions. Its viewport and session stay
alive; focusing it restores visibility before activation. Partially overlapping
and tiled windows stay visible together. `main_windows.parked` reports this state.

`main_windows.perf` in `ui.inspect` provides diagnostic focus-command and per-view render
counts, the most recent `root_ui_ms` / `secondary_ui_ms`, and `visible_mix_strips`.
The UI timings measure CPU layout work, excluding GPU presentation and event-loop waiting.
The existing `frame_ms` remains an interval estimate and includes idle time; it is not an
input-latency benchmark. The Mixer borrows session snapshots and paints only visible strips.

In the Edit window, two-finger scrolling pans the timeline horizontally and the tracks vertically,
including when the pointer is over a ruler or track header. Pinching zooms the timeline continuously
around the pointer, preserving the sample underneath it within the zoom and timeline-start limits.
Shift+scroll pans horizontally; Cmd+scroll (Ctrl off macOS) uses the same anchored zoom.
Natural scroll direction and momentum come from the platform's input stream. Moving the timeline
away from the playhead during playback pauses automatic following until the playhead is brought
back into view or the scrolling mode is reselected.

Agents can also call `view.zoom_at {"factor": 1.25, "anchor_px": 200}` through `engine.execute`;
`anchor_px` is measured from the timeline's left edge. `ui.scroll` and `ui.zoom` exercise the actual
pointer-input path, while `view.scroll` and `view.zoom_at` directly control saved view state.

The docked MIDI editor uses the same two-finger, diagonal, momentum, Shift+scroll and
pinch/Cmd+scroll gestures over the piano roll, piano keys, header and velocity lane. Horizontal scrolling
pans time; vertical scrolling pans pitch with subpixel precision. Notes, hit testing, the grid and
velocity stems share the same horizontal viewport. The editor opens with the whole clip visible;
zoom in to pan through it, and use **Fit** to restore the whole-clip view. Zoom stays between 1×
and 1024×, time panning stays within the clip, and pitch panning stays within MIDI notes 0–127.
Playback follow and centered modes can show blank time before or after the clip. Gestures leave
the Edit timeline, notes, undo history and session dirty status unchanged; stopped Center Playhead
panning intentionally changes the separate play-start cue.

With a selected MIDI clip and an open, rendered MIDI editor, agents can also call
`ui.midi_zoom_at {"factor": 1.25, "anchor_px": 200}`,
`ui.midi_scroll {"by_px": 20, "by_y_px": 9}`, or `ui.midi_fit {}` through `engine.execute`.
The anchor is measured from the piano roll's left edge; positive pan values move the viewport
toward later ticks and lower pitches. The returned view contains `zoom`, `scroll_ticks` and
`top_pitch`. MIDI view state persists when the dock is closed/reopened; selecting another clip
starts with that clip fitted and its notes centred vertically.

### Native-menu auxiliary windows

On macOS the desktop host presents right-click menus and dropdowns with AppKit. The menu descriptions and
command actions are shared with egui fallback hosts. Native tracking never owns an app-state
borrow; cancelling a menu executes nothing, and replacing the session invalidates its result.
Dropdowns anchor below their controls. Ordinary form choices return a row identity and apply
once on the next frame, after validating the session, row label and result age. Other hosts
retain their existing egui UI.

These UI-only commands expose the auxiliary controls used by native menus:

| Command | Parameters | Result |
|---|---|---|
| `window.plugin_search` | `{track: id, slot: index, instruments?: false, scan?: "CLAP" or "VST3"}` | Opens a focused plugin picker with an empty search. With `instruments: true`, the track must be an instrument track and `slot` is ignored. `scan` explicitly scans the requested format before showing results. |
| `window.send_controls` | `{track: id, slot: index}` | Opens the existing detailed controls for an assigned Send. |
| `window.routing_search` | `{track: id, input?: false, send?: index}` | Opens a focused bus-search picker for the input/output route, or for a Send when `send` is present. |
| `window.save_preset` | `{track: id, slot: index}` | Opens Save Preset controls for an occupied insert slot. |

Invalid or missing targets and out-of-range slots return an error. Choosing a plugin uses the
same insertion command, Undo history and automatic editor opening as the inline picker.
When opened from a native menu, auxiliary controls belong to that menu's viewport; a programmatic
call opens it in the root viewport.

### MIDI Editor playback and scrolling

The playback cursor is clipped to the piano roll and stops above the Velocity lane. It uses the shared transport position,
converted through the session tempo map into the clip's tick coordinates. `ui.midi_scrolling {}`
inspects the current choice and effective mode; pass `{"mode":"page"}` to change it. The same
command backs the MIDI Editor's **Scrolling** menu. It is UI preference state, independent of
`options.scrolling`, and defaults to `page` for old preferences.

| Mode | Behavior |
|---|---|
| `none` | Cursor moves; viewport stays in place. |
| `after_playback` | Viewport moves to the final playback location after Stop, Pause or natural completion, if offscreen. |
| `page` | Cursor moves; reaching the right edge advances the viewport by a page. Loop wraps and seeks can return to earlier pages. |
| `continuous` | Viewport moves past the centered playback cursor. Playback uses the Timeline selection. |
| `center` | Centered blue cursor (red when recording). While stopped, horizontal panning selects a separate playback cue. |
| `follow_edit` | Resolves the current `options.scrolling` mode, including changes made during playback. |

Manual horizontal pan/zoom during playback holds automatic follow. Reselecting the scrolling mode,
starting playback again, or bringing an offscreen cursor back into view resumes follow. Stopped
Center Playhead pans claim the MIDI Editor's play start; selecting time in the Edit window releases
that ownership. Transport toolbar buttons preserve it. Normal Play/Space and Half-Speed use the
cue; explicit `transport.play {"from":...}` and Play Edit take precedence. Loop playback, loop
recording and punch selections retain their ranges. Recording without a punch selection uses the
cue. Closing the editor, switching its target clip, or replacing the session releases its ownership.

`transport.set_play_start {"at":...}` sets the session's stopped playhead without replacing its
Edit or Timeline selection, creating an undo step or adding to the replay journal. Positions use
the standard formats below and clamp to the supported session-time range. The command alone
does not seek running playback; use `transport.locate` for that. `ui.inspect.midi_layout` includes
`scrolling`, `effective_scrolling`, `follow_hold`, `owns_transport`, `play_start` and `playhead_sample`.

**2026-10-14:** Added MIDI playback cursor, six independent scrolling modes and Center Playhead cue control.
**2026-10-14:** Restricted the cursor to the piano roll so it does not extend into the Velocity lane.

### MIDI Velocity lane height

Drag the horizontal divider between the piano roll and Velocity to change the lane height.
Dragging upward enlarges Velocity; dragging downward gives more space to the notes.
Double-click the divider to restore 64 points. When the divider has keyboard focus,
Up/Down adjusts its height by 8 points. The same control works in docked and floating editors.

`ui.midi_velocity_height {"height":120}` sets the preferred height in logical window points;
omit `height` to inspect it. Finite values clamp to 24–4096 points. The displayed height also
fits the editor, reserving at least 72 points for notes when space permits. Shrinking the editor
does not overwrite the preference. UI preferences retain the height across editor closure,
clip changes and app restarts; old preferences default to 64. Resizing does not edit notes,
selections, undo history or transport cues. The playback cursor still ends at the piano roll.

`ui.inspect.midi_layout` includes `velocity_divider` (the draggable rectangle) and
`velocity_height` (the preferred height), alongside the actual `velocity` rectangle.

**2026-10-14:** Added resizable Velocity with saved height, keyboard input and shared UI command.

Each Velocity stem has a horizontal tail that spans the note's duration. Tails track the
current time zoom and pan, and are clipped to the lane. Grab any part of a tail and drag
vertically to change that note's velocity; grabbing near a tail retains the pointer offset
so the value does not jump. Selected notes are drawn last in white; when
handles coincide, the selected note takes precedence.

Double-click a Velocity stem or tail to reset that note to 100 (the default for new notes).
Stems that share a start time are distinguished by the closest handle or tail.
Dragging keeps its original target when crossing another handle. Both gestures dispatch
`midi.note_edit`, preserving the note's pitch, timing and the lane height; Undo restores
the edited velocity. Double-clicking empty space does not edit notes.

**2026-10-14:** Added Velocity stem double-click reset and stable overlapping-stem targeting.
**2026-10-14:** Added note-length Velocity tails with direct drag/reset input and selection visibility.

- Tracks: id (number) or name (`"Kick"`).
- Clips: id (from `session.inspect`).
- Positions: samples (`96000`), seconds (`{"seconds": 2.5}`), or a time string in any timebase,
  optionally prefixed: `"bars_beats:5|1|000"`, `"min_secs:0:12.500"`, `"timecode:00:00:10:00"`,
  `"samples:48000"`.
- Ranges: `start` + `end` (or `start` + `length`); omitted = the current edit selection.

## Examples

```sh
P=$(soundcraft --demo --control 0 | sed -n 's/SOUNDCRAFT_CONTROL_PORT=//p')   # or read stderr
soundcraft-cli app --port $P mix.solo '{"track":"Bass"}'
soundcraft-cli app --port $P transport.play
soundcraft-cli app --port $P ui.screenshot '{"path":"/tmp/shot.png"}'
soundcraft-cli app --port $P ui.menu.invoke '{"path":"Track > New..."}'
```

Offscreen (no window): `cargo run -p soundcraft-ui-egui --example ui_shot -- out.png script.jsonl`,
where each script line is a request (`{"method": …, "params": …}`), `{"shot": "path.png"}` or
`{"steps": n}`. Use `--system-theme light|dark|none` to supply the initial OS appearance, and
script events such as `{"system_theme": "dark"}` to change it on subsequent frames.

Appearance defaults to Dark. `ui.theme {"mode":"system"}` follows the appearance supplied by
the native or web integration at launch and on later frames, while the saved choice stays System.
When the integration cannot report an appearance, as on Linux, System reads the desktop's
settings portal (`org.freedesktop.appearance` `color-scheme`) and follows its changes; without
a portal answer it uses Dark. Manual Light and Dark stay fixed when the OS appearance changes.
An offscreen render without `--system-theme` supplies no OS appearance and therefore resolves
System to Dark, or on Linux to the desktop's portal setting when one is available.

`view.scroll_into_view {"track":"Bass"}` (a track id or name; without it, the first selected
track) brings the track to the top of the Edit window and the left of the Mix window, the same as
Scroll Into View in the track name menu. A window not on screen catches up when it is next shown.

## Panel docking

Tracks and Groups, Clips, MIDI Editor, Memory Locations, and Undo History use movable panels. Commands target the active Edit or Mix view; floating panels stay inside the current viewport. Edit and Mix retain separate arrangements; Clips and MIDI Editor belong to Edit. Transport, plug-in, and specialist utility windows retain their existing hosts. The document/session view is protected.

The Edit window starts with Tracks and Groups in a 168-point column in a dock at the left edge, Clips in a 230-point column in a dock at the right edge, and the MIDI Editor under the session view between them. Both docks run the window's full height. Mix starts with Tracks and Groups docked at its left edge. Memory Locations and Undo History float. A dock column stacks tab groups top to bottom: a panel dropped on a group's middle joins it as a tab, on its upper or lower third stacks a new group above or below it, and at a column's edge makes a new column. A panel dropped at an empty window edge makes a dock there. Layouts, active tabs, panel visibility, and floating rectangles are saved with the app's existing UI preferences. These operations do not edit document contents.

Panel IDs: `tracks`, `clips`, `midi`, `memory`, `history`. The protected center is `canvas`.

`ui.dock` accepts `operation` and `panel`. Operations are `open`, `close`, `activate`, `float` (with finite positive `rect: [x, y, width, height]`), or `move` (with `target` and `zone: tab|center|left|right|top|bottom|above|below`; PascalCase names such as `Left` also parse). `open` and `activate` show a hidden panel at its saved location and bring a visible panel to the front. A panel with no saved location, or whose saved anchor cannot be restored, opens at its default place: Tracks and Groups and Clips join their dock's first group, or make that dock again at its default width when it is gone; the MIDI Editor opens under the session view; Memory Locations and Undo History float at their default rectangles. Center and `tab` group with the target; side zones split beside it; `above` and `below` stack the panel in a new group above or below a floating target's group. `ui.dock.reset` restores the default arrangement. Invalid moves return an error without changing the layout.

Invoke these UI commands through `engine.execute`, for example `{"command":"ui.dock","params":{"operation":"float","panel":"tracks","rect":[100,100,320,440]}}`.

Advanced `ui.dock` operations use the same validated transaction as pointer gestures:
`moveFloating` updates `rect`, `resizeSplit` accepts a boolean child `path` and
`size` (`{"Ratio":0.5}`, `{"FixedFirst":280}`, or `{"FixedSecond":280}`),
`setStackOpen` takes `open`, and `resizeStack` takes a point `height` or null.
A center `move` can include `before` (panel ID or null to append) for tab order.
The equivalent `{"action": ...}` form accepts a serialized shared docking action;
this is also the form emitted by the renderer. Protected-canvas and app-specific panel
rules apply equally to both forms.

The saved UI preferences and window configurations carry `docking_version`. Settings with no
version, or an older one, hold the split-frame layouts of earlier releases: they load with the
default Edit and Mix layouts and keep every other setting.
