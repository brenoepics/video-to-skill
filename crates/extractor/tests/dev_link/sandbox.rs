//! A throwaway HOME and checkout for `scripts/dev-link.sh` to act on.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// The real script and SKILL.md, as committed at the workspace root.
pub fn workspace_root() -> PathBuf {
    // CARGO_MANIFEST_DIR = crates/extractor → workspace root is two levels up.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .to_path_buf()
}

pub struct Sandbox {
    pub home: PathBuf,
    pub checkout: PathBuf,
    /// The only PATH entry besides the system tools: empty, so `claude`
    /// and `cargo` are absent however the host is set up, until a test
    /// plants a fake `claude` there.
    bin: PathBuf,
}

impl Sandbox {
    /// A fresh HOME beside a copy of the checkout: the real script, the
    /// real SKILL.md, and a fake `target/release/vts-extract` that reports
    /// `version`.
    pub fn new(name: &str, version: &str) -> Sandbox {
        let root = std::env::temp_dir()
            .join("vts-dev-link-tests")
            .join(format!("{name}-{}", std::process::id()));
        if root.exists() {
            fs::remove_dir_all(&root).unwrap();
        }
        let home = root.join("home");
        let checkout = root.join("checkout");
        let bin = root.join("bin");
        for dir in [&home, &checkout.join("scripts"), &bin] {
            fs::create_dir_all(dir).unwrap();
        }
        let real = workspace_root();
        fs::copy(
            real.join("scripts/dev-link.sh"),
            checkout.join("scripts/dev-link.sh"),
        )
        .unwrap();
        fs::copy(real.join("SKILL.md"), checkout.join("SKILL.md")).unwrap();
        let release = checkout.join("target/release");
        fs::create_dir_all(&release).unwrap();
        write_executable(
            &release.join("vts-extract"),
            &format!(
                "#!/bin/sh\n\
                 case \"$1\" in\n\
                 --version) echo \"vts-extract {version}\" ;;\n\
                 *) echo \"unexpected: $*\" >&2; exit 64 ;;\n\
                 esac\n"
            ),
        );
        Sandbox {
            home,
            checkout,
            bin,
        }
    }

    /// Replace the copied SKILL.md's pin so it disagrees with the binary.
    pub fn pin_skill_to(&self, version: &str) {
        let path = self.checkout.join("SKILL.md");
        let text = fs::read_to_string(&path).unwrap();
        let repinned = text.replace(
            &format!("requires vts-extract {}", env!("CARGO_PKG_VERSION")),
            &format!("requires vts-extract {version}"),
        );
        assert_ne!(text, repinned, "SKILL.md must carry the workspace version");
        fs::write(path, repinned).unwrap();
    }

    /// A `claude` on PATH that records where and with what it was started.
    pub fn plant_fake_claude(&self) {
        write_executable(
            &self.bin.join("claude"),
            "#!/bin/sh\n\
             printf 'cwd=%s\\nVTS_DATA_DIR=%s\\nargs=%s\\n' \
             \"$PWD\" \"${VTS_DATA_DIR-unset}\" \"$*\" > \"$HOME/claude-run\"\n",
        );
    }

    pub fn claude_run(&self) -> String {
        fs::read_to_string(self.home.join("claude-run")).unwrap()
    }

    pub fn skill_link(&self) -> PathBuf {
        self.home.join(".claude/skills/video-to-skill")
    }

    pub fn npx_install(&self) -> PathBuf {
        self.home.join(".agents/skills/video-to-skill")
    }

    pub fn data_dir(&self) -> PathBuf {
        self.home.join(".vts-dev")
    }

    /// Run the script from HOME with `VTS_DATA_DIR` unset and a PATH that
    /// holds only the system tools and the sandbox's own bin.
    pub fn run(&self, args: &[&str]) -> Output {
        Command::new("sh")
            .arg(self.checkout.join("scripts/dev-link.sh"))
            .args(args)
            .current_dir(&self.home)
            .env_clear()
            .env("HOME", &self.home)
            .env("PATH", format!("{}:/usr/bin:/bin", self.bin.display()))
            .output()
            .unwrap()
    }
}

pub fn write_executable(path: &Path, body: &str) {
    fs::write(path, body).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
}

/// Whether this process may create a directory symlink here at all; on
/// Windows that is a policy (Developer Mode or a privilege), not a given.
pub fn symlinks_available(dir: &Path) -> bool {
    let target = dir.join("symlink-probe-target");
    let link = dir.join("symlink-probe-link");
    fs::create_dir_all(&target).unwrap();
    let _ = fs::remove_file(&link);
    #[cfg(unix)]
    let made = std::os::unix::fs::symlink(&target, &link).is_ok();
    #[cfg(windows)]
    let made = std::os::windows::fs::symlink_dir(&target, &link).is_ok();
    #[cfg(not(any(unix, windows)))]
    let made = false;
    made && fs::read_link(&link).is_ok()
}

pub fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

pub fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}
