// Modified for EsanCraft; original notices retained. See MODIFICATIONS.md.
//! Baking Clip Effects must preserve the clip's timing and final samples, including Undo/Redo
//! and session bundles, rather than permanently storing the lookahead delay in its new source.

use serde_json::json;
use soundcraft_audio_io::{AudioBuffer, FileFormat};
use soundcraft_engine::{Engine, io};
use soundcraft_model::{ChannelFormat, Clip, Session, TrackKind};
use soundcraft_time::Range;

#[test]
fn rendered_clip_effects_preserve_timing_tail_undo_and_saved_audio() {
    let mut s = Session::default();
    let track = s.add_track(TrackKind::Audio, ChannelFormat::Stereo, None);
    let mut signal = vec![0.0; 8192 + 7];
    for at in [7, 4103, 8198] {
        signal[at] = 0.25;
    }
    let audio = vec![signal.clone(), signal];
    let source = io::add_source(&mut s, "original", AudioBuffer { sample_rate: 48_000, channels: audio }, None, FileFormat::Wav);
    let clip = s.new_clip_id();
    s.track_mut(track).unwrap().playlist_mut().unwrap().clips.push(Clip::audio(clip, "edited", source, 7, 123, 8192));
    let mut e = Engine::new(s);
    let range = Range::new(0, 8315);
    let expected = soundcraft_mix::render_range(e.session(), range, 512);
    e.execute("clip.effects_set", &json!({"clip": clip.0, "params": {"comp.lookahead": 5.0, "comp.ratio": 1.0}})).unwrap();
    assert_eq!(soundcraft_mix::render_range(e.session(), range, 512), expected);
    let before = e.session().clone();
    e.execute("clip.effects_render", &json!({"clip": clip.0})).unwrap();
    let rendered = e.session().find_clip(clip).unwrap().1;
    assert_eq!((rendered.start, rendered.length, rendered.source_offset()), (123, 8192, 0));
    let rendered_audio = e.session().pool.get(rendered.source().unwrap()).unwrap();
    assert_eq!(rendered_audio.frames(), 8192);
    for c in &rendered_audio.buffer.channels {
        let impulses: Vec<_> = c.iter().enumerate().filter(|(_, v)| v.abs() > 1e-6).map(|(i, _)| i).collect();
        assert_eq!(impulses, [0, 4096, 8191]);
    }
    assert_eq!(soundcraft_mix::render_range(e.session(), range, 512), expected);
    assert!(e.undo());
    assert_eq!(e.session(), &before);
    assert!(e.redo());
    assert_eq!(soundcraft_mix::render_range(e.session(), range, 512), expected);
    let (bytes, _) = io::session_bundle(e.session()).unwrap();
    let mut reopened = Engine::default();
    io::open_session_bytes(&mut reopened, "clip-effects.scraftz", &bytes).unwrap();
    assert_eq!(soundcraft_mix::render_range(reopened.session(), range, 512), expected);
}
