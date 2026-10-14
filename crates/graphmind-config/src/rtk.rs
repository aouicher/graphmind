//! Run curl through rtk when it's installed, so rtk records the call without filtering its output.

use std::io;
use std::path::Path;
use std::process::{Command, Output};

/// Runs `curl <args>`: through rtk's pass-through subcommand when rtk is on PATH, else directly.
/// Falls back to plain curl when rtk can't run curl at all, so an odd rtk never breaks the call.
pub fn curl(args: &[&str]) -> io::Result<Output> {
    curl_with(crate::paths::find_on_path("rtk").as_deref(), args)
}

fn curl_with(rtk: Option<&Path>, args: &[&str]) -> io::Result<Output> {
    if let Some(rtk) = rtk {
        let subcommand = version_output(rtk).map_or("proxy", |v| passthrough_subcommand(&v));
        if let Ok(output) = Command::new(rtk).arg(subcommand).arg("curl").args(args).output() {
            if !rtk_failed_to_run_curl(&output) {
                return Ok(output);
            }
        }
    }
    Command::new("curl").args(args).output()
}

fn version_output(rtk: &Path) -> Option<String> {
    let output = Command::new(rtk).arg("--version").output().ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// rtk 1.0 renamed `proxy` to `bypass` and kept `proxy` as an alias; rtk 0.x only has `proxy`,
/// so it's the answer whenever the version can't be read.
fn passthrough_subcommand(version_output: &str) -> &'static str {
    match major_version(version_output) {
        Some(major) if major >= 1 => "bypass",
        _ => "proxy",
    }
}

/// The major version in `rtk --version` output (`rtk 1.0.0`): the first word shaped like
/// `<major>.<minor>…`, with an optional leading `v`.
fn major_version(version_output: &str) -> Option<u64> {
    version_output.split_whitespace().find_map(|word| {
        let (major, rest) = word.strip_prefix('v').unwrap_or(word).split_once('.')?;
        if !rest.starts_with(|c: char| c.is_ascii_digit()) {
            return None;
        }
        major.parse().ok()
    })
}

/// rtk exits 126 or 127 when it can't run the command it's given (rtk 0.x takes `bypass` for a
/// program name), and has no exit code when a signal kills it. curl never exits 126 or 127, so
/// curl's own failures (offline, HTTP errors) aren't retried.
fn rtk_failed_to_run_curl(output: &Output) -> bool {
    matches!(output.status.code(), None | Some(126) | Some(127))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rtk_1_and_later_use_bypass_older_or_unreadable_versions_use_proxy() {
        for (output, subcommand) in [
            ("rtk 0.51.0\n", "proxy"),
            ("rtk 0.52.0", "proxy"),
            ("rtk 1.0.0\n", "bypass"),
            ("rtk 1.2.3-rc.1", "bypass"),
            ("rtk v2.0.0", "bypass"),
            ("rtk 10.4.1 (abc1234 2026-10-13)", "bypass"),
            ("rtk 0.52.0 (built 2026-10-13)", "proxy"),
            ("", "proxy"),
            ("rtk", "proxy"),
            ("rtk unknown", "proxy"),
            ("garbage 1. x.y", "proxy"),
        ] {
            assert_eq!(passthrough_subcommand(output), subcommand, "{output:?}");
        }
    }

    #[cfg(unix)]
    fn fake_rtk(dir: &Path, version: &str) -> std::path::PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let rtk = dir.join("rtk");
        let log = dir.join("calls.log");
        // Like rtk 0.x: `proxy` runs the command; anything else is taken for a program name.
        let script = format!(
            "#!/bin/sh\n\
             [ \"$1\" = --version ] && {{ echo 'rtk {version}'; exit 0; }}\n\
             echo \"$*\" >> '{log}'\n\
             [ \"$1\" = proxy ] && {{ shift; exec \"$@\"; }}\n\
             exit 127\n",
            log = log.display()
        );
        std::fs::write(&rtk, script).unwrap();
        std::fs::set_permissions(&rtk, std::fs::Permissions::from_mode(0o755)).unwrap();
        // A test thread that forked while the script was open for writing keeps it busy until
        // its child execs; running it fails with ETXTBSY (26 on Linux and macOS) until then.
        // Once one run starts, nothing can hold it open for writing any more.
        for _ in 0..200 {
            match Command::new(&rtk).arg("--version").output() {
                Err(e) if e.raw_os_error() == Some(26) => std::thread::sleep(std::time::Duration::from_millis(10)),
                _ => break,
            }
        }
        rtk
    }

    #[cfg(unix)]
    #[test]
    fn rtk_0_x_runs_curl_through_proxy() {
        let dir = tempfile::tempdir().unwrap();
        let rtk = fake_rtk(dir.path(), "0.52.0");
        let output = curl_with(Some(&rtk), &["--version"]).unwrap();
        assert!(output.status.success());
        assert!(String::from_utf8_lossy(&output.stdout).starts_with("curl "));
        assert_eq!(std::fs::read_to_string(dir.path().join("calls.log")).unwrap(), "proxy curl --version\n");
    }

    #[cfg(unix)]
    #[test]
    fn an_rtk_that_cannot_run_curl_falls_back_to_plain_curl() {
        let dir = tempfile::tempdir().unwrap();
        // Claims 1.0 but only knows `proxy`, so `bypass` exits 127.
        let rtk = fake_rtk(dir.path(), "1.0.0");
        let output = curl_with(Some(&rtk), &["--version"]).unwrap();
        assert!(output.status.success(), "plain curl ran");
        assert!(String::from_utf8_lossy(&output.stdout).starts_with("curl "));
        assert_eq!(std::fs::read_to_string(dir.path().join("calls.log")).unwrap(), "bypass curl --version\n");
    }

    #[cfg(unix)]
    #[test]
    fn only_rtk_failing_to_run_curl_triggers_the_fallback() {
        use std::os::unix::process::ExitStatusExt;
        let output = |status| Output { status, stdout: Vec::new(), stderr: Vec::new() };
        let exited = |code: i32| output(std::process::ExitStatus::from_raw(code << 8));
        assert!(rtk_failed_to_run_curl(&exited(127)), "command not found");
        assert!(rtk_failed_to_run_curl(&exited(126)), "not executable");
        assert!(rtk_failed_to_run_curl(&output(std::process::ExitStatus::from_raw(9))), "killed");
        for curl_code in [0, 6, 7, 22, 28, 56] {
            assert!(!rtk_failed_to_run_curl(&exited(curl_code)), "curl exit {curl_code}");
        }
    }

    #[test]
    fn without_rtk_curl_runs_directly() {
        let output = curl_with(None, &["--version"]).unwrap();
        assert!(output.status.success());
    }
}
