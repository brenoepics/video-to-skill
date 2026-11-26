//! The reference runner (`scripts/idm-runner.py`) against stand-ins for
//! torch, transformers and qwen-vl-utils: it hands the model the clip as
//! a plain absolute path, and only its JSON answer reaches stdout even
//! when a library prints there. Skips where `python3` is missing.

// Integration-test file: everything here is test code, where unwrap is fine.
#![allow(clippy::unwrap_used)]
#![cfg(unix)]

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const RUNNER: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../scripts/idm-runner.py");

const QWEN_VL_UTILS: &str = r#"
import json, os
def process_vision_info(messages, return_video_kwargs=False):
    with open(os.environ["STUB_LOG"], "a") as log:
        log.write(json.dumps(messages) + "\n")
    return None, ["video"], {}
"#;

const TRANSFORMERS: &str = r#"
import os
class _Ids:
    shape = (1, 3)
class _Inputs(dict):
    def to(self, device):
        return self
class _Tokenizer:
    padding_side = "right"
class AutoProcessor:
    @classmethod
    def from_pretrained(cls, path):
        processor = cls()
        processor.tokenizer = _Tokenizer()
        return processor
    def apply_chat_template(self, messages, tokenize, add_generation_prompt):
        return "prompt"
    def __call__(self, **kwargs):
        return _Inputs(input_ids=_Ids())
    def batch_decode(self, ids, skip_special_tokens):
        return [os.environ["STUB_ANSWER"]]
class _Generated:
    def __getitem__(self, key):
        return self
class Qwen2_5_VLForConditionalGeneration:
    device = "cpu"
    @classmethod
    def from_pretrained(cls, path, **kwargs):
        print("Loading checkpoint shards: 100%")
        os.write(1, b"native library noise\n")
        return cls()
    def generate(self, **kwargs):
        print("generating")
        return _Generated()
"#;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("vts-idm-runner-py-tests")
        .join(format!("{name}-{}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn has_python() -> bool {
    Command::new("python3")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

fn stubs(dir: &Path) -> PathBuf {
    let stubs = dir.join("stubs");
    fs::create_dir_all(&stubs).unwrap();
    fs::write(stubs.join("torch.py"), "").unwrap();
    fs::write(stubs.join("qwen_vl_utils.py"), QWEN_VL_UTILS).unwrap();
    fs::write(stubs.join("transformers.py"), TRANSFORMERS).unwrap();
    stubs
}

#[test]
fn the_reference_runner_sends_plain_paths_and_keeps_stdout_for_json() {
    if !has_python() {
        eprintln!("skipping: no python3");
        return;
    }
    let dir = scratch("plain-paths");
    let log = dir.join("messages.jsonl");
    let clip = dir.join("bundle").join("actions").join("w000.mp4");
    let batch = serde_json::json!({
        "protocol": 1,
        "stage": "detect",
        "model_dir": dir.join("model"),
        "requests": [{
            "id": "w000", "clip": clip, "fps": 2.0,
            "resized_width": 308, "resized_height": 252,
            "system": "system turn", "prompt": "user turn", "max_new_tokens": 256
        }]
    });

    let mut child = Command::new("python3")
        .arg(RUNNER)
        .env("PYTHONPATH", stubs(&dir))
        .env("STUB_LOG", &log)
        .env("STUB_ANSWER", "[]")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(batch.to_string().as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();

    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let answers: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!("{e}: {}", String::from_utf8_lossy(&out.stdout));
    });
    assert_eq!(
        answers,
        serde_json::json!({"responses": [{"id": "w000", "text": "[]"}]})
    );
    let messages: serde_json::Value =
        serde_json::from_str(fs::read_to_string(&log).unwrap().trim()).unwrap();
    let video = &messages[1]["content"][0];
    assert_eq!(video["type"], "video");
    assert_eq!(
        video["video"],
        clip.to_str().unwrap(),
        "a plain path: FFmpeg reads `file://` + a path as a different file"
    );
    assert_eq!(video["fps"], 2.0);
}
