//! Synthesise the placeholder audio under `assets/audio`: soft effects and
//! a gentle looping pond pad. Pure std — a WAV is just a RIFF header plus
//! 16-bit PCM samples, so there is nothing to depend on.
//!
//! ```sh
//! cargo run -p game --example gen_placeholder_audio
//! ```

use std::f32::consts::TAU;
use std::io::Write;
use std::path::PathBuf;

const SAMPLE_RATE: u32 = 22_050;

fn main() -> std::io::Result<()> {
    let root = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/audio"));
    let sfx = root.join("sfx");
    let music = root.join("music");
    std::fs::create_dir_all(&sfx)?;
    std::fs::create_dir_all(&music)?;

    write_wav(&sfx.join("hop.wav"), &hop())?;
    write_wav(&sfx.join("bump.wav"), &bump())?;
    write_wav(&sfx.join("chirp.wav"), &chirp())?;
    write_wav(&sfx.join("fanfare.wav"), &fanfare())?;
    write_wav(&music.join("pond.wav"), &pond())?;
    println!("wrote placeholder audio to {}", root.display());
    Ok(())
}

/// A soft tick for a completed hop.
fn hop() -> Vec<f32> {
    thud(660.0, 520.0, 0.07, 35.0, 0.5)
}

/// A low thud for a hedge bump.
fn bump() -> Vec<f32> {
    thud(150.0, 90.0, 0.09, 28.0, 0.6)
}

/// A phase-integrated decaying sweep.
fn thud(f0: f32, f1: f32, dur_secs: f32, decay: f32, gain: f32) -> Vec<f32> {
    let len = (SAMPLE_RATE as f32 * dur_secs) as usize;
    let mut phase = 0.0_f32;
    (0..len)
        .map(|s| {
            let t = s as f32 / SAMPLE_RATE as f32;
            let freq = f0 + (f1 - f0) * (t / dur_secs);
            phase += TAU * freq / SAMPLE_RATE as f32;
            gain * phase.sin() * (-decay * t).exp()
        })
        .collect()
}

/// A bright rising chirp with a slow vibrato — pitched up per clutch
/// position via playback speed.
fn chirp() -> Vec<f32> {
    let dur = 0.14_f32;
    let len = (SAMPLE_RATE as f32 * dur) as usize;
    let mut phase = 0.0_f32;
    (0..len)
        .map(|s| {
            let t = s as f32 / SAMPLE_RATE as f32;
            let x = t / dur;
            let freq = 1300.0 + 900.0 * x + 120.0 * (TAU * 40.0 * t).sin();
            phase += TAU * freq / SAMPLE_RATE as f32;
            // Hann-style window: silent at both ends.
            let window = (1.0 - (2.0 * x - 1.0).powi(2)).max(0.0);
            0.45 * phase.sin() * window
        })
        .collect()
}

/// A rising bell arpeggio — C5 E5 G5 C6 — for the level fanfare.
fn fanfare() -> Vec<f32> {
    let notes = [523.25, 659.25, 783.99, 1046.5];
    let spacing = 0.16_f32;
    let ring = 0.9_f32;
    let total = spacing * (notes.len() - 1) as f32 + ring;
    let len = (total * SAMPLE_RATE as f32) as usize;
    let mut out = vec![0.0_f32; len];
    for (i, &f) in notes.iter().enumerate() {
        let start = (spacing * i as f32 * SAMPLE_RATE as f32) as usize;
        let note_len = (ring * SAMPLE_RATE as f32) as usize;
        for s in 0..note_len {
            let t = s as f32 / SAMPLE_RATE as f32;
            // Fast attack, long bell ring, one inharmonic partial.
            let env = (-4.0 * t).exp() * (t / 0.01).min(1.0);
            out[start + s] += 0.3
                * env
                * ((TAU * f * t).sin()
                    + 0.4 * (TAU * 2.0 * f * t).sin()
                    + 0.2 * (TAU * 2.76 * f * t).sin());
        }
    }
    out
}

/// The looping pond bed: four slow chords on a circular timeline, each
/// fading in and out so the loop wraps cleanly. Oscillator frequencies are
/// quantised to whole cycles of the loop, so every sine is phase-continuous
/// across the wrap.
fn pond() -> Vec<f32> {
    const CHORD_SECS: f32 = 4.8;
    // C major, A minor, F major, G major — warm pads, one octave shimmer.
    let chords: [[f32; 4]; 4] = [
        [130.81, 196.0, 329.63, 523.25],
        [110.0, 164.81, 261.63, 440.0],
        [87.31, 174.61, 220.0, 349.23],
        [98.0, 196.0, 246.94, 392.0],
    ];
    let gains = [0.16, 0.10, 0.07, 0.04];
    let chord_len = (CHORD_SECS * SAMPLE_RATE as f32) as usize;
    let total = chord_len * chords.len();
    let loop_freq = SAMPLE_RATE as f32 / total as f32;
    let attack = 0.6_f32;
    (0..total)
        .map(|s| {
            let t = s as f32 / SAMPLE_RATE as f32;
            let u = (s % chord_len) as f32 / SAMPLE_RATE as f32;
            // Distance to the nearest edge of this chord's span, as a fade.
            let window = ((u.min(CHORD_SECS - u)) / attack).min(1.0);
            let swell = 1.0 + 0.08 * (TAU * 0.1 * t).sin();
            chords
                .iter()
                .zip(gains)
                .map(|(chord, gain)| {
                    chord
                        .iter()
                        .map(|&f| {
                            let f = (f / loop_freq).round() * loop_freq;
                            gain * (TAU * f * t).sin()
                        })
                        .sum::<f32>()
                })
                .sum::<f32>()
                * window
                * swell
        })
        .collect()
}

/// Write 16-bit mono PCM WAV.
fn write_wav(path: &std::path::Path, samples: &[f32]) -> std::io::Result<()> {
    let mut bytes = Vec::with_capacity(44 + samples.len() * 2);
    bytes.extend_from_slice(b"RIFF");
    // Cast to u32: usize's to_le_bytes would write 8 bytes on 64-bit.
    bytes.extend_from_slice(&((36 + samples.len() * 2) as u32).to_le_bytes());
    bytes.extend_from_slice(b"WAVE");
    bytes.extend_from_slice(b"fmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM
    bytes.extend_from_slice(&1u16.to_le_bytes()); // mono
    bytes.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    bytes.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes()); // byte rate
    bytes.extend_from_slice(&2u16.to_le_bytes()); // block align
    bytes.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&((samples.len() * 2) as u32).to_le_bytes());
    for &sample in samples {
        bytes.extend_from_slice(&((sample.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    std::fs::File::create(path)?.write_all(&bytes)
}
