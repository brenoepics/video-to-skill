//! A private desktop for the AT-SPI tests: a session bus, an X server
//! and a GTK probe window of the test's own, so nothing on the host's
//! real session is read or disturbed.
//!
//! Each piece is located first. When one is missing the test is skipped
//! with the reason — unless `VTS_REQUIRE_ATSPI` is set (the Linux CI job
//! sets it after installing them), in which case a missing piece fails.

use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

/// The GTK 3 probe: a window with a menu bar, a checkbox, an entry, a
/// password entry holding `argv[3]` and a button, each with known state,
/// then a line on stdout once shown.
const PROBE: &str = r#"
import sys
import gi
gi.require_version("Gtk", "3.0")
from gi.repository import GLib, Gtk

GLib.set_prgname(sys.argv[1])
window = Gtk.Window(title=sys.argv[2])
box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
bar = Gtk.MenuBar()
file_entry = Gtk.MenuItem(label="File")
menu = Gtk.Menu()
menu.append(Gtk.MenuItem(label="Open"))
save = Gtk.MenuItem(label="Save")
save.set_sensitive(False)
menu.append(save)
file_entry.set_submenu(menu)
bar.append(file_entry)
box.pack_start(bar, False, False, 0)
wrap = Gtk.CheckButton(label="Wrap lines")
wrap.set_active(True)
box.pack_start(wrap, False, False, 0)
entry = Gtk.Entry()
entry.set_text("hello")
entry.get_accessible().set_name("Greeting")
box.pack_start(entry, False, False, 0)
secret = Gtk.Entry()
secret.set_text(sys.argv[3])
secret.set_visibility(False)
secret.get_accessible().set_name("Password")
box.pack_start(secret, False, False, 0)
box.pack_start(Gtk.Button(label="Save copy"), False, False, 0)
window.add(box)
window.connect("destroy", Gtk.main_quit)
window.show_all()
print("ready", flush=True)
Gtk.main()
"#;

/// Where at-spi2-core installs the session service that starts the
/// accessibility bus on demand.
const A11Y_BUS_SERVICE: &str = "/usr/share/dbus-1/services/org.a11y.Bus.service";

pub fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("vts-a11y-linux-tests")
        .join(format!("{name}-{}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    for sub in ["runtime", "home"] {
        fs::create_dir_all(dir.join(sub)).unwrap();
    }
    dir
}

fn on_path(name: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

/// A Python that can import GTK 3 through `PyGObject` (a pyenv or
/// toolchain `python3` ahead of the system one often cannot).
fn gtk_python() -> Option<PathBuf> {
    ["python3", "python3.12", "python3.13", "python3.11"]
        .iter()
        .filter_map(|name| on_path(name))
        .chain([PathBuf::from("/usr/bin/python3")])
        .find(|python| {
            Command::new(python)
                .args(["-c", "import gi; gi.require_version('Gtk', '3.0')"])
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|status| status.success())
        })
}

fn missing<T>(what: &str) -> Option<T> {
    let required = std::env::var_os("VTS_REQUIRE_ATSPI").is_some_and(|v| !v.is_empty());
    assert!(!required, "VTS_REQUIRE_ATSPI is set but {what}");
    eprintln!("skipping the live AT-SPI test: {what}");
    None
}

/// How long a helper may take to print its first line.
const STARTUP_TIMEOUT: Duration = Duration::from_secs(20);

/// Start `command` and return it with the first line it prints; a helper
/// that prints nothing in time is killed and fails the test instead of
/// hanging it.
fn spawn_reading_line(mut command: Command) -> (Child, BufReader<ChildStdout>, String) {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        let read = stdout.read_line(&mut line);
        let _ = sender.send((read.map(|_| line), stdout));
    });
    let Ok((Ok(line), stdout)) = receiver.recv_timeout(STARTUP_TIMEOUT) else {
        child.kill().ok();
        child.wait().ok();
        panic!("{command:?} printed nothing within {STARTUP_TIMEOUT:?}");
    };
    (child, stdout, line.trim().to_string())
}

pub struct Desktop {
    pub dir: PathBuf,
    pub session: String,
    display: Option<String>,
    python: Option<PathBuf>,
    /// Started processes and their stdout, stopped in reverse on drop.
    children: Vec<(Child, BufReader<ChildStdout>)>,
}

impl Desktop {
    /// A session bus only: no X server, no accessibility bus.
    pub fn bus_only(name: &str) -> Option<Self> {
        Self::start(name, false)
    }

    /// A session bus, an X server and a Python that can show the probe.
    pub fn with_display(name: &str) -> Option<Self> {
        Self::start(name, true)
    }

    fn start(name: &str, display: bool) -> Option<Self> {
        let Some(daemon) = on_path("dbus-daemon") else {
            return missing("dbus-daemon is not installed");
        };
        let mut desktop = Self {
            dir: scratch(name),
            session: String::new(),
            display: None,
            python: None,
            children: Vec::new(),
        };
        if display {
            let Some(xvfb) = on_path("Xvfb") else {
                return missing("Xvfb is not installed");
            };
            if !Path::new(A11Y_BUS_SERVICE).exists() {
                return missing("at-spi2-core (the org.a11y.Bus service) is not installed");
            }
            let Some(python) = gtk_python() else {
                return missing("no python3 with PyGObject and GTK 3 (python3-gi, gir1.2-gtk-3.0)");
            };
            desktop.python = Some(python);
            let mut command = Command::new(xvfb);
            command.args([
                "-displayfd",
                "1",
                "-nolisten",
                "tcp",
                "-screen",
                "0",
                "800x600x24",
            ]);
            let (child, stdout, number) = spawn_reading_line(command);
            desktop.children.push((child, stdout));
            desktop.display = Some(format!(":{number}"));
        }
        let mut command = Command::new(daemon);
        command.args(["--session", "--nofork", "--print-address=1"]);
        desktop.environment(&mut command);
        let (child, stdout, address) = spawn_reading_line(command);
        desktop.children.push((child, stdout));
        desktop.session = address;
        Some(desktop)
    }

    /// The private session's environment, for anything started in it.
    pub fn environment(&self, command: &mut Command) {
        command
            .env("XDG_RUNTIME_DIR", self.dir.join("runtime"))
            .env("HOME", self.dir.join("home"))
            .env_remove("AT_SPI_BUS_ADDRESS")
            .env_remove("NO_AT_BRIDGE");
        if !self.session.is_empty() {
            command.env("DBUS_SESSION_BUS_ADDRESS", &self.session);
        }
        match &self.display {
            Some(display) => command.env("DISPLAY", display).env("GDK_BACKEND", "x11"),
            None => command.env_remove("DISPLAY"),
        };
    }

    /// Show the probe window; returns its pid once it is on screen.
    pub fn launch_probe(&mut self, prgname: &str, title: &str, secret: &str) -> u32 {
        let script = self.dir.join("probe.py");
        fs::write(&script, PROBE).unwrap();
        let mut command = Command::new(self.python.as_ref().unwrap());
        command.arg(&script).args([prgname, title, secret]);
        self.environment(&mut command);
        let (child, stdout, line) = spawn_reading_line(command);
        assert_eq!(line, "ready", "the probe window did not come up");
        let pid = child.id();
        self.children.push((child, stdout));
        pid
    }

    /// `vts-extract observe <args>` inside the private session.
    pub fn observe(&self, args: &[&str]) -> (i32, String, String) {
        self.observe_with(args, &[])
    }

    /// The same, with `extra` variables set on top of the session's.
    pub fn observe_with(&self, args: &[&str], extra: &[(&str, &str)]) -> (i32, String, String) {
        let mut command = Command::new(env!("CARGO_BIN_EXE_vts-extract"));
        command.arg("observe").args(args);
        self.environment(&mut command);
        command.envs(extra.iter().copied());
        let output = command.output().unwrap();
        (
            output.status.code().unwrap(),
            String::from_utf8_lossy(&output.stdout).into_owned(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
        )
    }
}

impl Drop for Desktop {
    fn drop(&mut self) {
        // The accessibility bus launcher exits with the session bus it
        // was activated on, taking the accessibility bus with it.
        while let Some((mut child, _)) = self.children.pop() {
            child.kill().ok();
            child.wait().ok();
        }
    }
}
