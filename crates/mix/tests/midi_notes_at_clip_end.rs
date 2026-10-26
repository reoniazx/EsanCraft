// Modified for EsanCraft; original notices retained. See MODIFICATIONS.md.
//! Terminal note-offs belong to the block beginning at the clip end, even though that clip no
//! longer overlaps the block. Check event timing as well as the rendered instrument's release.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soundcraft_dsp::{Plugin, PluginInfo, Transport};
use soundcraft_midi::{Note, Sequence};
use soundcraft_mix::{INSTRUMENT_SLOT, MixEngine, PreparedInstance, render_range};
use soundcraft_model::{ChannelFormat, Clip, Insert, Session, TrackId, TrackKind};
use soundcraft_time::Range;
use std::sync::{Arc, Mutex};

const HOSTED_ID: &str = "clap:soundcraft-note-end-test";
type Events = Arc<Mutex<Vec<(i64, u8, bool)>>>;

/// Records note events at their absolute timeline samples, without loading an external plugin.
struct NoteEvents {
    events: Events,
    pos: i64,
}

impl Plugin for NoteEvents {
    fn info(&self) -> &'static PluginInfo {
        soundcraft_dsp::plugin_info("subtractive_synth").unwrap()
    }
    fn prepare(&mut self, _: f32, _: usize, _: usize) {}
    fn reset(&mut self) {}
    fn set_param(&mut self, _: &str, _: f32) -> bool {
        false
    }
    fn param(&self, _: &str) -> Option<f32> {
        None
    }
    fn set_transport(&mut self, transport: &Transport) {
        self.pos = transport.samples;
    }
    fn note_on(&mut self, offset: usize, pitch: u8, _: u8) {
        self.events.lock().unwrap().push((self.pos + offset as i64, pitch, true));
    }
    fn note_off(&mut self, offset: usize, pitch: u8) {
        self.events.lock().unwrap().push((self.pos + offset as i64, pitch, false));
    }
    fn process(&mut self, io: &mut [Vec<f32>], frames: usize) {
        for channel in io {
            channel.iter_mut().take(frames).for_each(|x| *x = 0.0);
        }
    }
}

fn add_note(s: &mut Session, track: TrackId, start: i64, end: i64, ticks: i64) {
    let mut sequence = Sequence::default();
    sequence.notes.push(Note { pitch: 60, velocity: 100, release_velocity: 64, channel: 0, start: 0, length: ticks });
    let id = s.new_clip_id();
    s.track_mut(track).unwrap().playlist_mut().unwrap().clips.push(Clip::midi(id, "note", start, end - start, sequence));
}

fn render_events(s: &Session, track: TrackId, block: usize, end: i64) -> Vec<(i64, u8, bool)> {
    let events: Events = Arc::default();
    let mut engine = MixEngine::new(48_000.0, block);
    engine.set_external_instances(true);
    engine.adopt(vec![PreparedInstance {
        track,
        slot: INSTRUMENT_SLOT,
        id: HOSTED_ID.into(),
        ch: 2,
        plugin: Box::new(NoteEvents { events: Arc::clone(&events), pos: 0 }),
    }]);
    let mut out = vec![vec![0.0; block]; 2];
    let mut pos = 0;
    while pos < end {
        let frames = block.min((end - pos) as usize);
        engine.render(s, pos, frames, &mut out);
        pos += frames as i64;
    }
    events.lock().unwrap().clone()
}

#[test]
fn terminal_note_off_keeps_its_exact_sample_in_every_block_size_and_track_offset() {
    // 960 ticks = 24 000 samples at 120 BPM / 48 kHz. Include a naturally ending note and a
    // longer note clipped at the region end, aligned and unaligned ends, and MIDI Track Offsets.
    for block in [1, 37, 64, 128, 512, 1024] {
        for clip_end in [2048, 2049, 24_000] {
            for offset in [-73, 0, 73] {
                let start = 256;
                let mut s = Session::default();
                let t = s.add_track(TrackKind::Instrument, ChannelFormat::Stereo, None);
                s.track_mut(t).unwrap().instrument = Some(Insert::new(HOSTED_ID));
                s.edit.values.insert(format!("midi.offset.{}", t.0), offset as f64);
                // The natural note end is start + 24 000; the last case reaches it exactly.
                let end = start + clip_end;
                add_note(&mut s, t, start, end, 960);
                let got = render_events(&s, t, block, end + offset + block as i64 + 1);
                assert_eq!(got, [(start + offset, 60, true), (end + offset, 60, false)], "block {block}, end {end}, offset {offset}");
            }
        }
    }
}

#[test]
fn adjacent_clips_release_the_old_note_before_starting_the_new_one_at_a_block_boundary() {
    for block in [64, 512, 1024] {
        let mut s = Session::default();
        let t = s.add_track(TrackKind::Instrument, ChannelFormat::Stereo, None);
        s.track_mut(t).unwrap().instrument = Some(Insert::new(HOSTED_ID));
        // Playlists need not be sorted: putting the later clip first must not put the old
        // clip's terminal note-off after the new clip's same-pitch note-on.
        add_note(&mut s, t, 2048, 4096, 960);
        add_note(&mut s, t, 0, 2048, 960);
        let got = render_events(&s, t, block, 4096 + block as i64);
        assert_eq!(got, [(0, 60, true), (2048, 60, false), (2048, 60, true), (4096, 60, false)], "block {block}");
    }
}

#[test]
fn bounced_synth_releases_after_aligned_and_unaligned_clip_ends() {
    for block in [37, 64, 512, 1024] {
        for end in [2048, 2049] {
            let mut s = Session::default();
            let t = s.add_track(TrackKind::Instrument, ChannelFormat::Stereo, None);
            let mut synth = Insert::new("subtractive_synth");
            synth.params.insert("amp_release".into(), 1.0);
            synth.params.insert("filter_release".into(), 1.0);
            s.track_mut(t).unwrap().instrument = Some(synth);
            add_note(&mut s, t, 0, end, 960);
            let out = render_range(&s, Range::new(0, 8192), block);
            assert!(out.iter().any(|ch| ch[..end as usize].iter().any(|v| v.abs() > 1e-3)), "the note must sound before ending");
            let peak = out.iter().flat_map(|ch| &ch[end as usize + 2048..]).fold(0.0f32, |p, v| p.max(v.abs()));
            assert!(peak < 1e-6, "block {block}, end {end}: the voice kept sounding after its release, peak {peak}");
        }
    }
}
