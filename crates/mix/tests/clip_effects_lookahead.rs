//! Clip Effects processes file audio ahead of playback, so lookahead never moves a clip or
//! discards its last samples. Check the shared playback/bounce and clip-rendering paths.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soundcraft_audio_io::AudioBuffer;
use soundcraft_mix::{MixEngine, render_clips, render_range, render_track_pre_fader};
use soundcraft_model::{ChannelFormat, Clip, FadeShape, Insert, Session, SourceAudio, SourceId, TrackId, TrackKind};
use soundcraft_time::Range;
use std::sync::Arc;

fn fixture(start: i64, length: usize, format: ChannelFormat) -> (Session, TrackId, u64) {
    let mut s = Session::default();
    s.edit.click = false;
    s.outputs[0].format = format;
    let mut signal = vec![0.0; length];
    for at in [0, length / 2, length - 1] {
        signal[at] = 0.25;
    }
    let channels = (0..format.channels()).map(|ch| signal.iter().map(|x| x * (ch + 1) as f32).collect()).collect();
    s.pool.insert(SourceId(900), Arc::new(SourceAudio::new(AudioBuffer { sample_rate: 48_000, channels })));
    let track = s.add_track(TrackKind::Audio, format, None);
    let id = s.new_clip_id();
    s.track_mut(track).unwrap().playlist_mut().unwrap().clips.push(Clip::audio(id, "impulses", SourceId(900), 0, start, length as i64));
    (s, track, id.0)
}

fn lookahead(s: &mut Session, clip: u64, ms: f64) {
    s.edit.values.insert(format!("clip_fx.{clip}.comp.lookahead"), ms);
    s.edit.values.insert(format!("clip_fx.{clip}.comp.ratio"), 1.0);
}

fn assert_audio(actual: &[Vec<f32>], expected: &[Vec<f32>], context: &str) {
    assert_eq!(actual.len(), expected.len(), "{context}: channels");
    for (ch, (a, b)) in actual.iter().zip(expected).enumerate() {
        assert_eq!(a.len(), b.len(), "{context}: channel {ch} length");
        for (i, (&a, &b)) in a.iter().zip(b).enumerate() {
            assert!((a - b).abs() < 1e-6, "{context}: channel {ch}, sample {i}: got {a}, expected {b}");
        }
    }
}

#[test]
fn first_and_last_samples_stay_on_the_timeline_in_every_render_block_size() {
    for format in [ChannelFormat::Mono, ChannelFormat::Stereo, ChannelFormat::Surround51] {
        for (start, length) in [(0, 8192), (123, 8193), (11, 17)] {
            let (plain, _, id) = fixture(start, length, format);
            let range = Range::new(0, start + length as i64 + 19);
            for block in [1, 37, 64, 512, 1024] {
                let expected = render_range(&plain, range, block);
                for ms in [0.0, 5.0, 10.0] {
                    let mut s = plain.clone();
                    lookahead(&mut s, id, ms);
                    let actual = render_range(&s, range, block);
                    assert_audio(&actual, &expected, &format!("{format:?}, start {start}, length {length}, block {block}, {ms} ms"));
                }
            }
        }
    }
}

#[test]
fn partial_bounces_and_clip_renders_keep_the_requested_interval() {
    let (plain, track, id) = fixture(123, 8192, ChannelFormat::Stereo);
    let mut s = plain.clone();
    lookahead(&mut s, id, 5.0);
    for range in [Range::new(123, 8315), Range::new(4219, 8315), Range::new(8314, 8315)] {
        for block in [37, 512, 1024] {
            assert_audio(&render_range(&s, range, block), &render_range(&plain, range, block), "partial bounce");
        }
        assert_audio(&render_clips(&s, track, range), &render_clips(&plain, track, range), "clip render / consolidate");
        assert_audio(&render_track_pre_fader(&s, track, range), &render_track_pre_fader(&plain, track, range), "commit / freeze");
    }
}

#[test]
fn overlapping_and_adjacent_clips_with_different_lookaheads_stay_aligned() {
    let (mut plain, track, first) = fixture(0, 1024, ChannelFormat::Stereo);
    let second = plain.new_clip_id();
    let third = plain.new_clip_id();
    let clips = &mut plain.track_mut(track).unwrap().playlist_mut().unwrap().clips;
    clips.push(Clip::audio(second, "overlap", SourceId(900), 0, 512, 1024));
    clips.push(Clip::audio(third, "adjacent", SourceId(900), 0, 1536, 1024));
    let mut s = plain.clone();
    lookahead(&mut s, first, 5.0);
    lookahead(&mut s, second.0, 10.0);
    lookahead(&mut s, third.0, 1.0);
    let range = Range::new(0, 2560);
    for block in [37, 512, 1024] {
        assert_audio(&render_range(&s, range, block), &render_range(&plain, range, block), "overlapping / adjacent clips");
    }
    assert_audio(&render_clips(&s, track, range), &render_clips(&plain, track, range), "consolidate overlapping clips");
}

#[test]
fn varying_callback_sizes_and_seek_reset_do_not_replay_delayed_audio() {
    let (mut s, _, id) = fixture(123, 8192, ChannelFormat::Stereo);
    lookahead(&mut s, id, 5.0);
    let mut eng = MixEngine::new(48_000.0, 512);
    let mut tmp = vec![vec![0.0; 512]; 2];
    let mut actual = vec![Vec::new(); 2];
    let mut pos = 0;
    for n in [1, 37, 128, 512].into_iter().cycle() {
        let n = n.min(8315 - pos);
        eng.render(&s, pos as i64, n, &mut tmp);
        for (o, b) in actual.iter_mut().zip(&tmp) {
            o.extend_from_slice(&b[..n]);
        }
        pos += n;
        if pos == 8315 {
            break;
        }
    }
    assert_audio(&actual, &render_range(&s, Range::new(0, 8315), 512), "varying callbacks");
    for reset in [false, true] {
        if reset {
            eng.reset();
        }
        eng.render(&s, 8314, 1, &mut tmp);
        let mut fresh = MixEngine::new(48_000.0, 512);
        let mut expected = vec![vec![0.0; 512]; 2];
        fresh.render(&s, 8314, 1, &mut expected);
        for (a, b) in tmp.iter().zip(&expected) {
            assert_eq!(a[0], b[0], "seek / reset last sample");
            assert!(a[0] > 0.2, "last sample must survive seeking");
        }
    }
}

#[test]
fn changing_lookahead_or_toggling_bypass_does_not_leave_old_delayed_samples() {
    let (mut s, _, id) = fixture(0, 8192, ChannelFormat::Stereo);
    lookahead(&mut s, id, 5.0);
    let mut eng = MixEngine::new(48_000.0, 512);
    let mut actual = vec![vec![0.0; 512]; 2];
    eng.render(&s, 0, 512, &mut actual);
    for (pos, ms, bypass) in [(512, 10.0, false), (1024, 10.0, true), (1536, 5.0, false), (4096, 0.0, false)] {
        lookahead(&mut s, id, ms);
        s.edit.set_flag(&format!("clip_fx.bypass.{id}"), bypass);
        eng.render(&s, pos, 512, &mut actual);
        let mut fresh = MixEngine::new(48_000.0, 512);
        let mut expected = vec![vec![0.0; 512]; 2];
        fresh.render(&s, pos, 512, &mut expected);
        assert_audio(&actual, &expected, "lookahead edit / bypass");
    }
}

#[test]
fn clip_lookahead_composes_with_track_insert_compensation() {
    let (mut plain, track, id) = fixture(0, 8192, ChannelFormat::Stereo);
    let mut insert = Insert::new("compressor");
    insert.params.insert("lookahead".into(), 5.0);
    insert.params.insert("ratio".into(), 1.0);
    plain.track_mut(track).unwrap().mixer.inserts[0] = Some(insert);
    let mut s = plain.clone();
    lookahead(&mut s, id, 10.0);
    for block in [37, 512, 1024] {
        assert_audio(&render_range(&s, Range::new(0, 8192), block), &render_range(&plain, Range::new(0, 8192), block), "clip + track lookahead");
    }
}

#[test]
fn offset_stretch_gain_and_linear_fades_are_preserved() {
    let (mut plain, track, id) = fixture(17, 8192, ChannelFormat::Stereo);
    let clip = &mut plain.track_mut(track).unwrap().playlist_mut().unwrap().clips[0];
    clip.content = soundcraft_model::ClipContent::Audio { source: SourceId(900), offset: 13 };
    clip.stretch = 1.25;
    clip.gain_db = -6.0;
    clip.fade_in.len = 128;
    clip.fade_in.shape = FadeShape::Linear;
    clip.fade_out.len = 256;
    clip.fade_out.shape = FadeShape::Linear;
    let mut s = plain.clone();
    lookahead(&mut s, id, 5.0);
    let range = Range::new(0, 8209);
    assert_audio(&render_range(&s, range, 512), &render_range(&plain, range, 512), "clip edits");
    assert_audio(&render_clips(&s, track, range), &render_clips(&plain, track, range), "clip render edits");
}

#[test]
fn compressed_clip_matches_the_effect_chain_with_its_delay_removed() {
    let (mut s, track, id) = fixture(0, 8192, ChannelFormat::Stereo);
    lookahead(&mut s, id, 5.0);
    s.edit.values.insert(format!("clip_fx.{id}.comp.threshold"), -30.0);
    s.edit.values.insert(format!("clip_fx.{id}.comp.ratio"), 4.0);
    s.edit.values.insert(format!("clip_fx.{id}.comp.attack"), 0.01);
    s.edit.values.insert(format!("clip_fx.{id}.eq.hpf_on"), 1.0);
    let mut eq = soundcraft_dsp::create("eq_7band").unwrap();
    eq.prepare(48_000.0, 512, 2);
    eq.set_param("hpf_on", 1.0);
    eq.reset();
    let mut comp = soundcraft_dsp::create("compressor").unwrap();
    comp.prepare(48_000.0, 512, 2);
    for (param, value) in [("lookahead", 5.0), ("threshold", -30.0), ("ratio", 4.0), ("attack", 0.01)] {
        comp.set_param(param, value);
    }
    comp.reset();
    let latency = comp.latency();
    let source = s.pool.get(SourceId(900)).unwrap();
    let mut expected = source.buffer.channels.clone();
    for c in &mut expected {
        c.resize(8192 + latency, 0.0);
    }
    for pos in (0..8192 + latency).step_by(512) {
        let n = (8192 + latency - pos).min(512);
        let mut block: Vec<Vec<f32>> = expected.iter().map(|c| c[pos..pos + n].to_vec()).collect();
        eq.process(&mut block, n);
        comp.process(&mut block, n);
        for (c, b) in expected.iter_mut().zip(&block) {
            c[pos..pos + n].copy_from_slice(b);
        }
    }
    for c in &mut expected {
        c.drain(..latency);
    }
    assert!(expected[1].iter().any(|x| x.abs() > 1e-4), "the effect still produces audio");
    assert!(expected[1][0].abs() < 0.5, "the compressor / EQ must actually process it");
    assert_audio(&render_clips(&s, track, Range::new(0, 8192)), &expected, "EQ + active compressor");
}
