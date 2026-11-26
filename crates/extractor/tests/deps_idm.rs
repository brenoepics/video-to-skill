//! The opt-in inverse-dynamics weights: pinned, checksum-verified file by
//! file, kept out of the default inventory.

// Integration-test file: everything here is test code, where unwrap is fine.
#![allow(clippy::unwrap_used)]

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use vts_extract::deps::bootstrap::Fetcher;
use vts_extract::deps::idm::{install_model, model_dir, model_ready};
use vts_extract::deps::registry::{self, idm_models, IdmFile, IdmModel};
use vts_extract::hashing::sha256_hex;

/// Serves canned bytes per URL and logs every request, with the size the
/// installer said to expect.
struct FakeHub {
    files: HashMap<String, Vec<u8>>,
    requested: Mutex<Vec<String>>,
    expected: Mutex<Vec<u64>>,
}

impl Fetcher for FakeHub {
    fn fetch(&self, url: &str, dest: &Path) -> anyhow::Result<()> {
        self.requested.lock().unwrap().push(url.to_string());
        let bytes = self
            .files
            .get(url)
            .ok_or_else(|| anyhow::anyhow!("404 {url}"))?;
        fs::write(dest, bytes)?;
        Ok(())
    }

    fn fetch_sized(&self, url: &str, dest: &Path, size: u64) -> anyhow::Result<()> {
        self.expected.lock().unwrap().push(size);
        self.fetch(url, dest)
    }
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("vts-idm-tests")
        .join(format!("{name}-{}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

/// A two-file stand-in model whose pins match `payloads`.
fn tiny_model(payloads: &[(&'static str, &[u8])]) -> IdmModel {
    let files: Vec<IdmFile> = payloads
        .iter()
        .map(|(name, bytes)| IdmFile {
            name,
            size: bytes.len() as u64,
            sha256: leak(sha256_hex(bytes)),
        })
        .collect();
    IdmModel {
        role: "detector",
        repo: "tests/tiny-idm",
        revision: "0123456789abcdef0123456789abcdef01234567",
        files: Box::leak(files.into_boxed_slice()),
    }
}

fn hub_for(model: &IdmModel, payloads: &[(&'static str, &[u8])]) -> FakeHub {
    let files = payloads
        .iter()
        .map(|(name, bytes)| {
            let file = model.files.iter().find(|f| f.name == *name).unwrap();
            (model.url(file), bytes.to_vec())
        })
        .collect();
    FakeHub {
        files,
        requested: Mutex::new(Vec::new()),
        expected: Mutex::new(Vec::new()),
    }
}

#[test]
fn both_idm_models_are_pinned_to_immutable_revisions_with_full_digests() {
    let [detector, parameterizer] = idm_models();
    assert_eq!(detector.repo, "xlangai/VideoAgentTrek-IDM-s1-7B");
    assert_eq!(
        detector.revision,
        "72486e686eb22418c8d36162097d10e15c70d1ea"
    );
    assert_eq!(parameterizer.repo, "xlangai/VideoAgentTrek-IDM-s2-7B");
    assert_eq!(
        parameterizer.revision,
        "bba5cb6531e5680c495c127f3eccbe7ae5d55d2b"
    );
    for model in [detector, parameterizer] {
        // Five bf16 shards plus the configs and tokenizer transformers
        // loads: 16.6 GB per model, as the Hugging Face API lists it.
        assert_eq!(model.files.len(), 14, "{}", model.repo);
        assert_eq!(model.total_bytes(), 16_595_962_740, "{}", model.repo);
        for file in model.files {
            let url = model.url(file);
            assert_eq!(
                url,
                format!(
                    "https://huggingface.co/{}/resolve/{}/{}",
                    model.repo, model.revision, file.name
                )
            );
            assert!(!url.contains("/resolve/main/"), "{url}");
            assert_eq!(file.sha256.len(), 64, "{url}");
            assert!(file.sha256.bytes().all(|b| b.is_ascii_hexdigit()));
        }
    }
    let shard = |model: &IdmModel| {
        model
            .files
            .iter()
            .find(|f| f.name == "model-00001-of-00005.safetensors")
            .unwrap()
            .sha256
    };
    assert_eq!(
        shard(&detector),
        "dd36014389956f0936b55ddb55827f11cbc351875c398395b0cffd418c75000e"
    );
    assert_eq!(
        shard(&parameterizer),
        "cfe0699106d158272eed379d763209e7baeafeb969cc8008f64ea2e35af69ff4"
    );
}

#[test]
fn the_default_inventory_never_includes_the_idm_weights() {
    for spec in registry::inventory() {
        assert!(
            !spec.url.contains("VideoAgentTrek"),
            "the 33 GB opt-in weights must stay out of `check --fix`: {}",
            spec.url
        );
    }
}

#[test]
fn a_model_installs_every_pinned_file_and_reads_ready() {
    let data = scratch("install");
    let payloads: [(&'static str, &[u8]); 2] =
        [("config.json", b"{}"), ("model.safetensors", b"weights")];
    let model = tiny_model(&payloads);
    let hub = hub_for(&model, &payloads);
    assert!(!model_ready(&model, &data));

    let dir = install_model(&model, &hub, &data).unwrap();

    assert_eq!(dir, model_dir(&model, &data));
    assert!(dir.starts_with(data.join("models")), "{}", dir.display());
    assert_eq!(fs::read(dir.join("model.safetensors")).unwrap(), b"weights");
    assert!(model_ready(&model, &data));

    assert_eq!(
        *hub.expected.lock().unwrap(),
        [2, 7],
        "each download is bounded by its own pinned size"
    );

    install_model(&model, &hub, &data).unwrap();
    assert_eq!(
        hub.requested.lock().unwrap().len(),
        2,
        "verified files are never fetched again"
    );
}

#[test]
fn an_interrupted_download_left_behind_is_swept_on_the_next_install() {
    let data = scratch("sweep");
    let payloads: [(&'static str, &[u8]); 2] =
        [("config.json", b"{}"), ("model.safetensors", b"weights")];
    let model = tiny_model(&payloads);
    let dir = model_dir(&model, &data);
    fs::create_dir_all(&dir).unwrap();
    let stale = dir.join(".model.safetensors.4242.0.download");
    fs::write(&stale, vec![0u8; 4096]).unwrap();
    let keep = dir.join("notes.txt");
    fs::write(&keep, "not ours").unwrap();

    install_model(&model, &hub_for(&model, &payloads), &data).unwrap();

    assert!(
        !stale.exists(),
        "a dead download must not keep its gigabytes"
    );
    assert!(keep.exists(), "only partial downloads are swept");
}

#[test]
fn a_file_with_the_wrong_digest_is_rejected_and_the_model_stays_not_ready() {
    let data = scratch("mismatch");
    let payloads: [(&'static str, &[u8]); 2] =
        [("config.json", b"{}"), ("model.safetensors", b"weights")];
    let model = tiny_model(&payloads);
    let tampered: [(&'static str, &[u8]); 2] =
        [("config.json", b"{}"), ("model.safetensors", b"tampered")];
    let hub = hub_for(&model, &tampered);

    let error = install_model(&model, &hub, &data).unwrap_err();

    assert!(
        format!("{error:#}").contains("checksum mismatch"),
        "{error:#}"
    );
    assert!(!model_ready(&model, &data));
    assert!(!model_dir(&model, &data).join("model.safetensors").exists());
}
