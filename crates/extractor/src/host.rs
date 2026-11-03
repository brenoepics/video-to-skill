//! Facts about the machine the extractor runs on that change how work
//! is scheduled. Detection is best-effort and never fails the pipeline.

use std::sync::OnceLock;

/// Where whisper inference lands on this machine, which decides how big
/// a model it can afford.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WhisperDevice {
    /// Bare-metal Apple Silicon: ggml's Metal backend.
    Metal,
    /// Everywhere else: Linux and Windows builds carry no GPU backend,
    /// Intel Macs are not worth a Metal-sized model, and a macOS virtual
    /// machine only sees a paravirtual GPU slower than its own CPU.
    Cpu,
}

/// The device this process routes whisper to, probed once and cached:
/// the model registry asks on every path lookup.
#[must_use]
pub fn whisper_device() -> WhisperDevice {
    static DEVICE: OnceLock<WhisperDevice> = OnceLock::new();
    *DEVICE.get_or_init(|| whisper_device_for(apple_silicon(), is_virtual_machine()))
}

/// Pure decision: Metal only on real Apple Silicon.
#[must_use]
pub fn whisper_device_for(apple_silicon: bool, virtual_machine: bool) -> WhisperDevice {
    if apple_silicon && !virtual_machine {
        WhisperDevice::Metal
    } else {
        WhisperDevice::Cpu
    }
}

const fn apple_silicon() -> bool {
    cfg!(all(target_os = "macos", target_arch = "aarch64"))
}

/// True when the kernel reports it is running under a hypervisor, or
/// the CPU brand string carries the "(Virtual)" marker Apple's
/// Virtualization framework exposes to guests. Virtual machines only
/// see a paravirtual GPU whose Metal lacks simdgroup reductions and
/// matrix multiply, so ggml's GPU path there is far slower than CPU.
#[must_use]
pub fn is_virtual_machine() -> bool {
    if !cfg!(target_os = "macos") {
        return false;
    }
    virtualized_from(
        sysctl("kern.hv_vmm_present").as_deref(),
        sysctl("machdep.cpu.brand_string").as_deref(),
    )
}

/// Pure decision over raw sysctl outputs; `None` means unreadable.
#[must_use]
pub fn virtualized_from(hv_vmm_present: Option<&str>, cpu_brand: Option<&str>) -> bool {
    let kernel_says_vm = hv_vmm_present.is_some_and(|v| v.trim() == "1");
    let brand_says_vm = cpu_brand.is_some_and(|b| b.contains("(Virtual)"));
    kernel_says_vm || brand_says_vm
}

fn sysctl(key: &str) -> Option<String> {
    command_stdout("sysctl", &["-n", key])
}

/// Stdout of `program args`, or `None` when it cannot run or fails.
pub(crate) fn command_stdout(program: &str, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new(program)
        .args(args)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::{virtualized_from, whisper_device, whisper_device_for, WhisperDevice};

    #[test]
    fn metal_is_for_bare_metal_apple_silicon_only() {
        assert_eq!(whisper_device_for(true, false), WhisperDevice::Metal);
        // A macOS VM only sees a paravirtual GPU slower than its CPU.
        assert_eq!(whisper_device_for(true, true), WhisperDevice::Cpu);
        // Linux, Windows and Intel Macs: no GPU backend worth routing to.
        assert_eq!(whisper_device_for(false, false), WhisperDevice::Cpu);
        assert_eq!(whisper_device_for(false, true), WhisperDevice::Cpu);
    }

    #[test]
    fn this_host_gets_metal_exactly_on_real_apple_silicon() {
        let apple_silicon = cfg!(all(target_os = "macos", target_arch = "aarch64"));
        assert_eq!(
            whisper_device(),
            whisper_device_for(apple_silicon, super::is_virtual_machine())
        );
    }

    #[test]
    fn hypervisor_flag_marks_a_virtual_machine() {
        assert!(virtualized_from(Some("1\n"), Some("Apple M1")));
    }

    #[test]
    fn apple_virtual_brand_marks_a_virtual_machine() {
        // GitHub-hosted macOS runners report the flag as 0 but brand
        // themselves "(Virtual)".
        assert!(virtualized_from(Some("0\n"), Some("Apple M1 (Virtual)\n")));
    }

    #[test]
    fn bare_metal_is_not_virtual() {
        assert!(!virtualized_from(Some("0\n"), Some("Apple M5\n")));
    }

    #[test]
    fn rosetta_brand_prefix_is_not_the_apple_vm_marker() {
        // Older macOS under Rosetta 2 reported "VirtualApple @ 2.50GHz" on
        // bare metal; only the parenthesised "(Virtual)" suffix means a VM.
        assert!(!virtualized_from(
            Some("0\n"),
            Some("VirtualApple @ 2.50GHz\n")
        ));
    }

    #[test]
    fn unreadable_sysctls_default_to_bare_metal() {
        assert!(!virtualized_from(None, None));
    }
}
