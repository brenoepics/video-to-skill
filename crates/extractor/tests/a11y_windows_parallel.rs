#![cfg(windows)]

//! Observations made from several threads at once must each end the way
//! a lone observation would. UI Automation clients created and torn down
//! concurrently once made one thread's factory calls fail with `E_FAIL`
//! ("creating a UI Automation condition failed") while other tests were
//! observing, so the backend serializes them; this keeps it that way.

// Integration-test file: everything here is test code, where unwrap is fine.
#![allow(clippy::unwrap_used)]

use std::thread;

use vts_extract::a11y::check::Check;
use vts_extract::a11y::observe::observe;

#[test]
fn parallel_observations_each_name_their_own_reason() {
    // `System` (pid 4) owns no window, so every observation walks COM,
    // the client, the cache request and the desktop's windows for real.
    let workers: Vec<_> = (0..4)
        .map(|_| {
            thread::spawn(|| {
                let checks = Check::parse_many(
                    r#"{"check":"window_title_contains","app":"System","text":"anything"}"#,
                )
                .unwrap();
                (0..5)
                    .map(|_| format!("{:#}", observe(&checks, false).unwrap_err()))
                    .collect::<Vec<_>>()
            })
        })
        .collect();

    for worker in workers {
        for text in worker.join().unwrap() {
            assert!(
                text.contains("no top-level window UI Automation can see"),
                "{text}"
            );
        }
    }
}
