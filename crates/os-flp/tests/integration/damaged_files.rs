// Copyright (C) 2026 ItsAlphaNeon
// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio

// spec: flp/container.md, flp/layouts.md
//
// Damaged project files: loading one is an error or a best-effort project, never a panic. Every case here panicked
// before it was fixed (an event stream with its mixer parameters before the mixer tracks, a playlist track record
// shorter than its track number), as found by feeding the loader mutated and random event streams.

use os_flp::{Chunk, ChunkBody, Event, FlpFile, Header, Tag, Value, project};

const VERSION: u8 = 199;
const ARR_START: u8 = 99;
const PL_TRACK: u8 = 238;
const CH_START: u8 = 64;
const MX_PARAMS: u8 = 225;
const MX_FLAGS: u8 = 236;

/// A file of `events` after a header of format 0, no channels and 96 ticks to the quarter note.
fn file(events: Vec<Event>) -> FlpFile {
    FlpFile {
        chunks: vec![
            Chunk { tag: Tag::HEADER, body: ChunkBody::Header(Header { raw: vec![0, 0, 1, 0, 96, 0] }) },
            Chunk { tag: Tag::DATA, body: ChunkBody::Events(events) },
        ],
        trailing: Vec::new(),
    }
}

fn version(text: &str) -> Event {
    let mut data = text.as_bytes().to_vec();
    data.push(0);
    Event::new(VERSION, Value::var(data))
}

/// Loads `file`, whatever comes of it, and checks that the loader is still standing; `what` names the case.
fn loads_or_errs(what: &str, file: &FlpFile) {
    let result = std::panic::catch_unwind(|| {
        let _ = project::load(file);
    });
    assert!(result.is_ok(), "{what}: loading panicked");
}

#[test]
fn a_playlist_track_record_shorter_than_its_number_does_not_panic() {
    // An older file (saved by FL 20.8) whose arrangement holds a track record of two bytes.
    let f = file(vec![version("20.8.0"), Event::new(ARR_START, Value::U16(0)), Event::new(PL_TRACK, Value::var(vec![1, 0]))]);
    loads_or_errs("short playlist track record", &f);
}

#[test]
fn mixer_parameters_before_the_mixer_tracks_do_not_panic() {
    for saved_by in ["26.0.0", "20.8.0", "11.5.1", "24.1.1"] {
        let f = file(vec![
            version(saved_by),
            Event::new(MX_PARAMS, Value::var(Vec::new())),
            Event::new(MX_FLAGS, Value::var(vec![0; 12])),
            Event::new(MX_FLAGS, Value::var(vec![0; 12])),
        ]);
        loads_or_errs(&format!("mixer parameters first, saved by {saved_by}"), &f);
    }
}

#[test]
fn mixer_parameters_between_the_mixer_tracks_do_not_panic() {
    // The parameters sit after the first track and before the second: the second track's events can't end at them.
    let f = file(vec![
        Event::new(CH_START, Value::U16(0)),
        Event::new(MX_FLAGS, Value::var(vec![0; 12])),
        Event::new(MX_PARAMS, Value::var(Vec::new())),
        Event::new(MX_FLAGS, Value::var(vec![0; 12])),
    ]);
    loads_or_errs("mixer parameters between two tracks", &f);
}

/// Event ids the loader reads, so random streams reach its code instead of stopping at the first unknown event.
const KNOWN_IDS: &[u8] = &[
    0, 9, 10, 11, 15, 17, 18, 20, 21, 23, 26, 30, 31, 32, 33, 34, 35, 36, 40, 41, 42, 43, 44, 45, 46, 48, 51,
    64, 65, 67, 69, 70, 71, 74, 75, 76, 80, 83, 85, 86, 89, 94, 95, 97, 98, 99, 100, 101, 103, 104, 128, 131,
    132, 135, 138, 139, 142, 143, 144, 145, 146, 147, 148, 149, 150, 154, 155, 156, 160, 161, 162, 164, 165,
    166, 167, 168, 193, 194, 195, 196, 197, 199, 201, 202, 203, 204, 205, 206, 207, 209, 212, 213, 215, 216,
    218, 219, 221, 223, 224, 225, 227, 228, 229, 230, 231, 233, 234, 235, 236, 237, 238, 239, 241, 242, 251,
];

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }

    fn byte(&mut self) -> u8 {
        self.next() as u8
    }
}

/// A random event stream in a valid container: mostly events the loader reads, with the sizes it expects.
fn random_stream(r: &mut Rng) -> Vec<u8> {
    const LENS: [usize; 15] = [0, 1, 2, 4, 8, 12, 16, 20, 24, 32, 64, 70, 100, 140, 200];
    const VERSIONS: [&str; 7] = ["9.0.0", "11.5.1", "12.3.0", "20.8.4", "21.0.0", "24.1.1", "24.2.0"];
    let mut ev = Vec::new();
    let v = VERSIONS[r.below(VERSIONS.len())];
    ev.extend_from_slice(&[VERSION, v.len() as u8 + 1]);
    ev.extend_from_slice(v.as_bytes());
    ev.push(0);
    for _ in 0..r.below(120) {
        let id = if r.below(10) < 8 { KNOWN_IDS[r.below(KNOWN_IDS.len())] } else { r.byte() };
        ev.push(id);
        match id {
            0..=63 => ev.push(r.byte()),
            64..=127 => ev.extend_from_slice(&[r.byte(), r.byte()]),
            128..=191 => ev.extend_from_slice(&[r.byte(), r.byte(), r.byte(), r.byte()]),
            _ => {
                let n = LENS[r.below(LENS.len())];
                ev.push(n as u8);
                for _ in 0..n {
                    ev.push(if r.below(4) == 0 { 0 } else { r.byte() });
                }
            }
        }
    }
    let mut out = b"FLhd".to_vec();
    out.extend_from_slice(&6u32.to_le_bytes());
    out.extend_from_slice(&[0, 0, 1, 0, 96, 0]);
    out.extend_from_slice(b"FLdt");
    out.extend_from_slice(&(ev.len() as u32).to_le_bytes());
    out.extend_from_slice(&ev);
    out
}

/// Flips, inserts, removes and cuts a few bytes.
fn damage(r: &mut Rng, bytes: &[u8]) -> Vec<u8> {
    let mut v = bytes.to_vec();
    for _ in 0..1 + r.below(6) {
        if v.is_empty() {
            return v;
        }
        let i = r.below(v.len());
        match r.below(5) {
            0 => v[i] = r.byte(),
            1 => v[i] ^= 1 << r.below(8),
            2 => {
                v.remove(i);
            }
            3 => v.insert(i, r.byte()),
            _ => v.truncate(i),
        }
    }
    v
}

#[test]
fn random_and_damaged_event_streams_never_panic() {
    let mut r = Rng(0x9E37_79B9_7F4A_7C15);
    for i in 0..30_000 {
        let seed = random_stream(&mut r);
        let bytes = if i % 2 == 0 { seed } else { damage(&mut r, &seed) };
        let Ok(parsed) = FlpFile::parse(&bytes) else { continue };
        if let Ok(written) = parsed.to_bytes() {
            assert_eq!(written, bytes, "stream {i}: parse then write changed the bytes");
        }
        let _ = os_flp::check_alignment(&parsed);
        loads_or_errs(&format!("stream {i} ({} bytes)", bytes.len()), &parsed);
    }
}
