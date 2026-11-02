//! Facts about the machine the extractor runs on that change how work
//! is scheduled. Detection is best-effort and never fails the pipeline.

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
    let brand_says_vm = cpu_brand.is_some_and(|b| b.to_ascii_lowercase().contains("virtual"));
    kernel_says_vm || brand_says_vm
}

fn sysctl(key: &str) -> Option<String> {
    let output = std::process::Command::new("sysctl")
        .args(["-n", key])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::virtualized_from;

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
    fn unreadable_sysctls_default_to_bare_metal() {
        assert!(!virtualized_from(None, None));
    }
}
