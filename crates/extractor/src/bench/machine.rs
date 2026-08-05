//! Machine identification for benchmark reports: chip name, logical
//! cores, and OS/architecture. Parsers are pure; only [`detect`] touches
//! the running system.

use super::Machine;

/// Identify the machine the benchmark is running on. Detection is
/// best-effort: an undetectable chip reports as "unknown".
#[must_use]
pub fn detect() -> Machine {
    Machine {
        chip: chip(),
        logical_cores: logical_cores(),
        os: format!("{}/{}", std::env::consts::OS, std::env::consts::ARCH),
    }
}

fn chip() -> String {
    if cfg!(target_os = "macos") {
        if let Some(chip) = command_stdout("sysctl", &["-n", "machdep.cpu.brand_string"])
            .as_deref()
            .and_then(parse_brand_string)
        {
            return chip;
        }
    }
    if cfg!(target_os = "linux") {
        if let Some(chip) = std::fs::read_to_string("/proc/cpuinfo")
            .ok()
            .as_deref()
            .and_then(parse_cpuinfo_model)
        {
            return chip;
        }
    }
    "unknown".to_string()
}

fn logical_cores() -> usize {
    std::thread::available_parallelism().map_or(0, std::num::NonZeroUsize::get)
}

/// Trimmed `sysctl -n machdep.cpu.brand_string` output; None when blank.
#[must_use]
pub fn parse_brand_string(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// First "model name" value from `/proc/cpuinfo` content.
#[must_use]
pub fn parse_cpuinfo_model(cpuinfo: &str) -> Option<String> {
    cpuinfo.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        (key.trim() == "model name")
            .then(|| value.trim().to_string())
            .filter(|v| !v.is_empty())
    })
}

fn command_stdout(program: &str, args: &[&str]) -> Option<String> {
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
    use super::{parse_brand_string, parse_cpuinfo_model};

    #[test]
    fn brand_string_is_trimmed() {
        assert_eq!(
            parse_brand_string("Apple M2 Pro\n").as_deref(),
            Some("Apple M2 Pro")
        );
    }

    #[test]
    fn blank_brand_string_is_rejected() {
        assert_eq!(parse_brand_string("  \n"), None);
    }

    #[test]
    fn cpuinfo_yields_the_first_model_name() {
        let cpuinfo = "processor\t: 0\n\
                       vendor_id\t: GenuineIntel\n\
                       model name\t: Intel(R) Xeon(R) CPU E5-2680 v4 @ 2.40GHz\n\
                       processor\t: 1\n\
                       model name\t: Something Else\n";

        assert_eq!(
            parse_cpuinfo_model(cpuinfo).as_deref(),
            Some("Intel(R) Xeon(R) CPU E5-2680 v4 @ 2.40GHz")
        );
    }

    #[test]
    fn cpuinfo_without_model_name_yields_none() {
        assert_eq!(parse_cpuinfo_model("processor\t: 0\nflags\t: fpu\n"), None);
    }

    #[test]
    fn cpuinfo_with_blank_model_name_yields_none() {
        assert_eq!(parse_cpuinfo_model("model name\t: \n"), None);
    }
}
