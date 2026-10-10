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
| `session.inspect` | `{detail?: "summary"|"full"}` | The document: tracks, clips (with times), inserts (+params when full), sends, routing, automation, markers, groups, busses, sources (`loaded` true when decoded audio is in the pool), selection, transport, undo label. |
| `ui.inspect` | `{}` | UI state (`window`, panels, dialogs), window size, playing/position, audio device, `edit_layout` (timeline and track row rects), and `midi_layout` (piano roll/velocity rects and MIDI viewport, for clicking). |
| `ui.set` | any `UiState` fields | e.g. `{"window": "Mix", "narrow_mix": true}`. |
| `ui.menu.list` | `{}` | Every catalog menu path and the command that implements it (or null). |
| `ui.menu.invoke` | `{path}` or `{id}` | Like clicking the menu item: may open its dialog (e.g. `"Track > New..."`). |
| `ui.click` | `{x, y, button?, count?, cmd?, shift?, alt?, ctrl?}` | Synthetic click (window points). |
| `ui.drag` | `{x, y, to_x, to_y, steps?}` | Synthetic drag with the primary button. |
| `ui.move` | `{x, y}` | Move the pointer. |
| `ui.scroll` | `{x, y, dx?, dy?, phase?, cmd?, shift?, alt?, ctrl?}` | Trackpad scrolling in window points. Positive deltas move content right/down. Omit `phase` for a complete gesture; use `start`, `move`, `end`, `cancel` for a sequence, including momentum. |
| `ui.zoom` | `{x, y, factor}` | Pinch at a window position; factor > 1 zooms in, < 1 zooms out (0.01–100). |
| `ui.key` | `{key, cmd?, shift?, alt?, ctrl?}` | Key press (`Space`, `Enter`, `A`, `F7`, `=`…). |
| `ui.text` | `{text}` | Type text into the focused field. |
| `ui.screenshot` | `{path?}` | PNG of the window (base64 in `png_base64` when no path). |
| `app.quit` | `{}` | Quit. |

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
Gestures in this editor leave the Edit timeline and the session document unchanged.

With a selected MIDI clip and an open, rendered MIDI editor, agents can also call
`ui.midi_zoom_at {"factor": 1.25, "anchor_px": 200}`,
`ui.midi_scroll {"by_px": 20, "by_y_px": 9}`, or `ui.midi_fit {}` through `engine.execute`.
The anchor is measured from the piano roll's left edge; positive pan values move the viewport
toward later ticks and lower pitches. The returned view contains `zoom`, `scroll_ticks` and
`top_pitch`. MIDI view state persists when the dock is closed/reopened; selecting another clip
starts with that clip fitted and its notes centred vertically.

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
When the integration cannot report an appearance, System uses Dark. Manual Light and Dark stay
fixed when the OS appearance changes. An offscreen render without `--system-theme` supplies no
OS appearance and therefore resolves System to Dark.
