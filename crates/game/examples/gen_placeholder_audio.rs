//! Synthesise the placeholder sound effects under `assets/audio/sfx`:
//! soft, watery one-shots. Pure std — a WAV is just a RIFF header plus
//! 16-bit PCM samples, so there is nothing to depend on. The music loop is
//! a hand-made take and is deliberately never touched by this generator.
//!
//! ```sh
//! cargo run -p game --example gen_placeholder_audio
//! ```

use std::f32::consts::TAU;
use std::io::Write;
use std::path::PathBuf;

const SAMPLE_RATE: u32 = 22_050;

fn main() -> std::io::Result<()> {
    let sfx = PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../assets/audio/sfx"
    ));
    std::fs::create_dir_all(&sfx)?;

    write_wav(&sfx.join("hop.wav"), &hop())?;
    write_wav(&sfx.join("bump.wav"), &bump())?;
    write_wav(&sfx.join("chirp.wav"), &chirp())?;
    write_wav(&sfx.join("fanfare.wav"), &fanfare())?;
    write_wav(&sfx.join("splash.wav"), &splash())?;
    write_wav(&sfx.join("flutter.wav"), &flutter())?;
    println!("wrote placeholder sfx to {}", sfx.display());
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

/// A watery splash: a pitch-dropping plop over a noise burst — crossing
/// onto or off a stream.
fn splash() -> Vec<f32> {
    let dur = 0.18_f32;
    let len = (SAMPLE_RATE as f32 * dur) as usize;
    let mut phase = 0.0_f32;
    let mut seed = 0x2545_F491_4F6C_DD1Du64;
    (0..len)
        .map(|s| {
            let t = s as f32 / SAMPLE_RATE as f32;
            let freq = 90.0 + 330.0 * (-t * 18.0).exp();
            phase += TAU * freq / SAMPLE_RATE as f32;
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let noise = ((seed >> 33) as i32) as f32 / i32::MAX as f32;
            0.35 * phase.sin() * (-t * 14.0).exp() + 0.18 * noise * (-t * 22.0).exp()
        })
        .collect()
}

/// A quick wing flutter: a soft buzz shaped by fast wingbeats — mum
/// taking off over a fallen log.
fn flutter() -> Vec<f32> {
    let dur = 0.22_f32;
    let len = (SAMPLE_RATE as f32 * dur) as usize;
    let mut seed = 0x6C_F00D_u64;
    (0..len)
        .map(|s| {
            let t = s as f32 / SAMPLE_RATE as f32;
            let x = t / dur;
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let noise = ((seed >> 33) as i32) as f32 / i32::MAX as f32;
            // Wingbeats: a full-wave-rectified ~34 Hz throb over a airy
            // tone-plus-noise bed, fading in and out.
            let beat = (TAU * 34.0 * t).sin().abs();
            let window = (1.0 - (2.0 * x - 1.0).powi(2)).max(0.0);
            0.4 * (0.6 * (TAU * 260.0 * t).sin() + 0.4 * noise) * beat * window
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
