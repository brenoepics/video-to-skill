//! A killed compiler must not leave its staging tree in an installable package.

#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use vts_extract::compile::compile;

#[path = "compile_recompile/fixture.rs"]
mod fixture;
use fixture::{bundle, files, procedure, scratch};

struct RunningCompiler(Child);

impl Drop for RunningCompiler {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn staging_tree(dir: &Path, prefix: &str) -> Option<PathBuf> {
    fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .find(|entry| entry.file_name().to_string_lossy().starts_with(prefix))
        .map(|entry| entry.path())
}

fn interrupt_compiler(bundle: &Path, ir_path: &Path, out: &Path) -> Option<PathBuf> {
    let mut child = RunningCompiler(
        Command::new(env!("CARGO_BIN_EXE_vts-extract"))
            .arg("compile")
            .arg("--bundle")
            .arg(bundle)
            .arg("--ir")
            .arg(ir_path)
            .arg("--out")
            .arg(out)
            .stdout(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let prefix = format!(".compile-{}-", child.0.id());
    let deadline = Instant::now() + Duration::from_secs(30);
    let orphan = loop {
        if let Some(path) = staging_tree(out, &prefix)
            .or_else(|| staging_tree(out.parent().unwrap(), &prefix))
            .filter(|path| path.join("SKILL.md").is_file())
        {
            break path;
        }
        if let Some(status) = child.0.try_wait().unwrap() {
            assert!(
                status.success(),
                "compiler failed before interruption: {status}"
            );
            return None;
        }
        assert!(
            Instant::now() < deadline,
            "compiler did not create staging in time"
        );
        thread::sleep(Duration::from_millis(2));
    };
    if let Err(error) = child.0.kill() {
        assert!(
            child.0.try_wait().unwrap().is_some_and(|s| s.success()),
            "could not interrupt compiler: {error}"
        );
        return None;
    }
    child.0.wait().unwrap();
    orphan.is_dir().then_some(orphan)
}

#[test]
fn interrupted_compilation_cannot_contaminate_the_recompiled_package() {
    let dir = scratch("interrupted-staging");
    let bundle = bundle(&dir);
    let out = dir.join("read-notes");
    let ir = procedure();
    compile(&ir, &bundle, &out).unwrap();
    let expected = files(&out);

    // Enough output files keep staging observable while the parent waits for
    // its creation. No delay or test-only hook is added to the compiler.
    let mut incoming = ir.clone();
    incoming.steps = (0..5000)
        .map(|i| {
            let mut step = ir.steps[0].clone();
            step.id = format!("step-{i:04}");
            step
        })
        .collect();
    let ir_path = dir.join("incoming.json");
    fs::write(&ir_path, serde_json::to_vec(&incoming).unwrap()).unwrap();
    // A scheduling pause can let a child finish normally before interruption.
    // Retry that case, but require a real orphan before checking containment.
    let orphan = (0..3).find_map(|_| {
        compile(&ir, &bundle, &out).unwrap();
        interrupt_compiler(&bundle, &ir_path, &out)
    });
    assert!(
        orphan.is_some(),
        "could not interrupt compilation during staging"
    );
    let orphan = orphan.unwrap();

    compile(&ir, &bundle, &out).unwrap();

    let actual = files(&out);
    let extra = actual.keys().find(|path| !expected.contains_key(*path));
    assert!(
        extra.is_none(),
        "an orphan staging file entered the package: {extra:?}"
    );
    assert_eq!(
        actual, expected,
        "interruption changed the recompiled package"
    );
    assert!(!orphan.starts_with(&out));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&orphan).unwrap().permissions().mode() & 0o077,
            0
        );
    }
}
