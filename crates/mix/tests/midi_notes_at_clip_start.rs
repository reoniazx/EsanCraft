//! A clip start is a sample position, not necessarily a whole tick; the MIDI notes in the clip
//! must still play in every block size (512-frame realtime blocks, 1024-frame bounce blocks),
//! exactly at their sample position and never before the clip start.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soundcraft_midi::{Note, Sequence};
use soundcraft_mix::{MixEngine, render_range};
use soundcraft_model::{ChannelFormat, Clip, Insert, Session, TrackKind};
use soundcraft_time::Range;

/// 20.48 ticks at 120 BPM / 48 kHz: the old integer-tick roundtrip turned this into sample 500.
const CLIP_START: i64 = 512;
const CLIP_LEN: i64 = 24_000;
/// 5 ms amp attack: the first sample above this is within a few hundred frames of the note-on.
const AUDIBLE: f32 = 1e-4;

fn session_clip_at(start: i64, note_tick: i64) -> Session {
    let mut s = Session::default(); // 48 kHz, 120 BPM → 25 samples per tick.
    let t = s.add_track(TrackKind::Instrument, ChannelFormat::Stereo, None);
    s.track_mut(t).unwrap().instrument = Some(Insert::new("subtractive_synth"));
    let mut seq = Sequence::default();
    seq.notes.push(Note { pitch: 60, velocity: 100, release_velocity: 64, channel: 0, start: note_tick, length: 960 });
    let cid = s.new_clip_id();
    s.track_mut(t).unwrap().playlist_mut().unwrap().clips.push(Clip::midi(cid, "m", start, start + CLIP_LEN, seq));
    s
}

fn first_audible(out: &[Vec<f32>]) -> Option<usize> {
    let len = out.first()?.len();
    (0..len).find(|&i| out.iter().any(|c| c.get(i).is_some_and(|x| x.abs() > AUDIBLE)))
}

#[test]
fn note_at_tick_aligned_clip_start_plays() {
    let s = session_clip_at(0, 0);
    let out = render_range(&s, Range::new(0, CLIP_LEN), 512);
    let at = first_audible(&out).expect("the note at sample 0 must sound");
    assert!(at < 480, "and right at the clip start: {at}");
}

#[test]
fn note_at_unaligned_clip_start_plays_in_512_frame_blocks() {
    let s = session_clip_at(CLIP_START, 0);
    let out = render_range(&s, Range::new(0, CLIP_START + CLIP_LEN), 512);
    let at = first_audible(&out).expect("the note at an unaligned clip start must sound in 512-frame blocks");
    assert!(at >= CLIP_START as usize, "it must never sound before the clip start: {at}");
    assert!(at < CLIP_START as usize + 480, "it must sound at the clip start, not a block late: {at}");
}

#[test]
fn note_at_unaligned_clip_start_plays_in_1024_frame_blocks() {
    let s = session_clip_at(CLIP_START, 0);
    let out = render_range(&s, Range::new(0, CLIP_START + CLIP_LEN), 1024);
    let at = first_audible(&out).expect("…and in 1024-frame blocks (bounces)");
    assert!(at >= CLIP_START as usize, "it must never sound before the clip start: {at}");
    assert!(at < CLIP_START as usize + 480, "it must sound at the clip start: {at}");
}

#[test]
fn note_beyond_the_first_block_keeps_its_sample_position() {
    // Tick 960 = 24 000 samples after the start, so sample 24 512 exactly. Integer ticks moved it
    // 12 samples early (tick 980 of the rounded clip start); fractional ticks keep it exact.
    let s = session_clip_at(CLIP_START, 960);
    let out = render_range(&s, Range::new(0, CLIP_START + CLIP_LEN + 4096), 512);
    let at = first_audible(&out).expect("the note must sound");
    let exact = (CLIP_START + 24_000) as usize;
    assert!(at >= exact, "it must not sound before its sample position {exact}: {at}");
    assert!(at < exact + 480, "it must sound at its sample position: {at}");
}

#[test]
fn realtime_engine_plays_the_note_at_the_unaligned_clip_start() {
    let s = session_clip_at(CLIP_START, 0);
    let mut eng = MixEngine::new(48_000.0, 512);
    let mut out = vec![vec![0.0f32; 512]; 2];
    let mut timeline = vec![0.0f32; 10 * 512];
    for b in 0..10usize {
        eng.render(&s, (b * 512) as i64, 512, &mut out);
        for (i, x) in out[0].iter().enumerate() {
            timeline[b * 512 + i] = *x;
        }
    }
    let at = timeline.iter().position(|x| x.abs() > AUDIBLE).expect("the realtime path must sound the note");
    assert!(at >= CLIP_START as usize, "it must never sound before the clip start: {at}");
    assert!(at < CLIP_START as usize + 480, "it must sound at the clip start: {at}");
}
