//! The VideoAgentTrek inverse-dynamics weights (Video2Action), opt-in only.
//!
//! Two Qwen2.5-VL-7B fine-tunes from xlangai on Hugging Face, Apache-2.0,
//! public and ungated: stage 1 (`IDM-s1`) finds the GUI actions in a
//! clip and when they happen, stage 2 (`IDM-s2`) names each one's
//! parameters (where it clicked, what it typed). Each model is five bf16
//! safetensors shards plus the configs and tokenizer `transformers`
//! loads: 16.6 GB apiece, 33.2 GB together, which is why they never join
//! the default inventory — `check --fix --actions` discloses them on
//! their own.
//!
//! Pinned to a commit. Shard digests are the git-lfs object ids the Hugging
//! Face API lists for that commit (an lfs oid is the file's sha256); the
//! small files are plain git blobs there, so their sha256 was taken by
//! downloading each at the pinned commit. The two repos share every small
//! file byte for byte and differ only in the shards.

/// One pinned file of a model repo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdmFile {
    pub name: &'static str,
    pub size: u64,
    pub sha256: &'static str,
}

/// A pinned model repo: every file `transformers` needs to load it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdmModel {
    /// `detector` (stage 1) or `parameterizer` (stage 2).
    pub role: &'static str,
    pub repo: &'static str,
    pub revision: &'static str,
    pub files: &'static [IdmFile],
}

impl IdmModel {
    /// The immutable download URL of `file` at the pinned commit.
    #[must_use]
    pub fn url(&self, file: &IdmFile) -> String {
        format!(
            "https://huggingface.co/{}/resolve/{}/{}",
            self.repo, self.revision, file.name
        )
    }

    #[must_use]
    pub fn total_bytes(&self) -> u64 {
        self.files.iter().map(|file| file.size).sum()
    }

    /// `repo@<short revision>`: what every event this model produced
    /// records as its source.
    #[must_use]
    pub fn label(&self) -> String {
        format!("{}@{}", self.repo, short(self.revision))
    }

    /// Directory name inside `models/`: the repo name and revision, so a
    /// re-pin installs beside the old weights instead of over them.
    #[must_use]
    pub fn dir_name(&self) -> String {
        let name = self.repo.rsplit('/').next().unwrap_or(self.repo);
        format!("{name}-{}", short(self.revision))
    }
}

fn short(revision: &str) -> &str {
    revision.get(..12).unwrap_or(revision)
}

const fn file(name: &'static str, size: u64, sha256: &'static str) -> IdmFile {
    IdmFile { name, size, sha256 }
}

/// Configs and tokenizer: identical in both repos.
const CHAT_TEMPLATE: IdmFile = file(
    "chat_template.json",
    1050,
    "ad60d90252ed0b0705ba14e2d0ad0fec0beac1ea955642b54059b36052d8bc96",
);
const CONFIG: IdmFile = file(
    "config.json",
    1374,
    "fca783c77eeeb29d82042e0b465e285201175ccfe11d1cf4aca64fa29156c7af",
);
const GENERATION_CONFIG: IdmFile = file(
    "generation_config.json",
    244,
    "ee519655686f42fee1560c63107ef1e1b40e7a98084c16733f7a5716b5d17a04",
);
const MERGES: IdmFile = file(
    "merges.txt",
    1_671_839,
    "599bab54075088774b1733fde865d5bd747cbcc7a547c5bc12610e874e26f5e3",
);
const INDEX: IdmFile = file(
    "model.safetensors.index.json",
    57_619,
    "73b333b0b16e5286ddba615d2caebcd495cf7e616f52eb217a81781393d79de9",
);
const PREPROCESSOR: IdmFile = file(
    "preprocessor_config.json",
    350,
    "f2058c716eef96ccaed1cc1e2d0c08306b62586d535b28d9d08e691b2fab7ca0",
);
const TOKENIZER: IdmFile = file(
    "tokenizer.json",
    7_031_645,
    "c0382117ea329cdf097041132f6d735924b697924d6f6fc3945713e96ce87539",
);
const TOKENIZER_CONFIG: IdmFile = file(
    "tokenizer_config.json",
    7226,
    "7d0fc4691628c9f6b0c77f8138a80517896a307e074dd5ac87e279ea85515774",
);
const VOCAB: IdmFile = file(
    "vocab.json",
    2_776_833,
    "ca10d7e9fb3ed18575dd1e277a2579c16d108e32f27439684afa0e10b1440910",
);

const SHARD_SIZES: [u64; 5] = [
    3_900_233_256,
    3_864_726_320,
    3_864_726_424,
    3_864_733_680,
    1_089_994_880,
];

const fn files(shards: [&'static str; 5]) -> [IdmFile; 14] {
    [
        CHAT_TEMPLATE,
        CONFIG,
        GENERATION_CONFIG,
        MERGES,
        file(
            "model-00001-of-00005.safetensors",
            SHARD_SIZES[0],
            shards[0],
        ),
        file(
            "model-00002-of-00005.safetensors",
            SHARD_SIZES[1],
            shards[1],
        ),
        file(
            "model-00003-of-00005.safetensors",
            SHARD_SIZES[2],
            shards[2],
        ),
        file(
            "model-00004-of-00005.safetensors",
            SHARD_SIZES[3],
            shards[3],
        ),
        file(
            "model-00005-of-00005.safetensors",
            SHARD_SIZES[4],
            shards[4],
        ),
        INDEX,
        PREPROCESSOR,
        TOKENIZER,
        TOKENIZER_CONFIG,
        VOCAB,
    ]
}

const DETECTOR_FILES: [IdmFile; 14] = files([
    "dd36014389956f0936b55ddb55827f11cbc351875c398395b0cffd418c75000e",
    "c423f81ed864f72ba18958eacd47d96df7189073fff91b0a61a738418d41195f",
    "1d51229c1580bf020b50efed54de6a8771ec9c0ea9cb2922affd2be9a2b6512b",
    "bf0ad048dd9226a0b87b38ebf82d81565e9de54423d11622f53f975c14479a00",
    "8406ec2c38de64d7ec30388c65851055b6b002d5d1675ee751946de1908cc885",
]);

const PARAMETERIZER_FILES: [IdmFile; 14] = files([
    "cfe0699106d158272eed379d763209e7baeafeb969cc8008f64ea2e35af69ff4",
    "d98928be49e8b3fb089d5e154eefa4936edf250de7d44f11dbd58a72a0df5f5a",
    "1aa18bcecace975645368ca429d2e764fb9f2b40941e0fa8e21a62fc9e59a1b7",
    "e1cc032c9764b36a8593575072577843db8c84eeec57f7fc109d7c44ed9da4de",
    "cac2607097dca466060fd3eb82faf49b39a37dba2108bdf99b8b4fe3241a23fe",
]);

/// Stage 1: GUI action types and their start and end times in a clip.
pub const IDM_DETECTOR: IdmModel = IdmModel {
    role: "detector",
    repo: "xlangai/VideoAgentTrek-IDM-s1-7B",
    revision: "72486e686eb22418c8d36162097d10e15c70d1ea",
    files: &DETECTOR_FILES,
};

/// Stage 2: one detected action's parameters, as `computer_use` calls.
pub const IDM_PARAMETERIZER: IdmModel = IdmModel {
    role: "parameterizer",
    repo: "xlangai/VideoAgentTrek-IDM-s2-7B",
    revision: "bba5cb6531e5680c495c127f3eccbe7ae5d55d2b",
    files: &PARAMETERIZER_FILES,
};

/// Both stages, in pipeline order.
#[must_use]
pub fn idm_models() -> [IdmModel; 2] {
    [IDM_DETECTOR, IDM_PARAMETERIZER]
}
