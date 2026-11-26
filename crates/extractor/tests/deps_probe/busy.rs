//! The PATH compatibility probe while executables are being written.
//!
//! On Linux, exec of a file that any process still holds open for writing
//! fails with `ETXTBSY`, and a child forked by another thread inherits every
//! open descriptor until its own exec. A probe that reads that as "unusable"
//! reports a perfectly good tool missing whenever one thread writes an
//! executable while another spawns one — which is exactly what parallel
//! tests, and users whose machine is installing a tool, do.

use std::io::Write;
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

use super::*;
use vts_extract::deps::status;

const SPAWNERS: usize = 6;
const ROUNDS: usize = 100;

/// One thread rewrites and probes its own script while others probe theirs
/// in a loop; every probe must still find a usable executable.
#[test]
fn probe_survives_executables_written_while_other_threads_spawn() {
    let root = scratch("busy-race");
    let data = scratch("busy-race-data");
    let barrier = Arc::new(Barrier::new(SPAWNERS + 1));

    let spawners = (0..SPAWNERS)
        .map(|i| {
            let bin = root.join(format!("spawner-{i}"));
            fake_executable(&bin, FFMPEG_FILE_NAME);
            let env = env_with(vec![bin], data.clone());
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                (0..ROUNDS)
                    .filter(|_| status(Tool::Ffmpeg, &env).presence != Presence::OnPath)
                    .count()
            })
        })
        .collect::<Vec<_>>();

    let writer_bin = root.join("writer");
    let env = env_with(vec![writer_bin.clone()], data);
    barrier.wait();
    let writer_misses = (0..ROUNDS)
        .filter(|_| {
            fake_executable(&writer_bin, FFMPEG_FILE_NAME);
            status(Tool::Ffmpeg, &env).presence != Presence::OnPath
        })
        .count();
    let spawner_misses = spawners
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .sum::<usize>();

    assert_eq!(
        (writer_misses, spawner_misses),
        (0, 0),
        "probes reported a usable executable missing (writer, spawners)"
    );
}

/// A script another thread still holds open for writing is retried until
/// the writer closes it, instead of being reported missing outright.
#[test]
fn probe_waits_for_a_script_still_open_for_writing() {
    let bin = scratch("busy-held-bin");
    let data = scratch("busy-held-data");
    let path = bin.join(FFMPEG_FILE_NAME);
    let mut file = fs::File::create(&path).unwrap();
    file.write_all(b"#!/bin/sh\n").unwrap();
    file.sync_all().unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    let writer = thread::spawn(move || {
        thread::sleep(Duration::from_millis(80));
        drop(file);
    });

    let found = status(Tool::Ffmpeg, &env_with(vec![bin.clone()], data));
    writer.join().unwrap();

    assert_eq!(found.presence, Presence::OnPath);
    assert_eq!(found.location, Some(path));
}
