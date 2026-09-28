use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, ExitStatus, Stdio};
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

const REPO: &str = "https://github.com/uxeric/ee.git";
const INSTALL_URL: &str = "https://raw.githubusercontent.com/uxeric/ee/HEAD/install.sh";
const CHECK_TIMEOUT: Duration = Duration::from_secs(8);

fn data_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .map(|d| d.join("eoe"))
}

pub fn installed_rev() -> Option<String> {
    let rev = std::fs::read_to_string(data_dir()?.join("installed-rev")).ok()?;
    let rev = rev.trim();
    (!rev.is_empty()).then(|| rev.to_string())
}

pub fn is_newer(installed: &str, remote: &str) -> bool {
    !remote.starts_with(installed) && !installed.starts_with(remote)
}

pub fn spawn_check() -> Option<Receiver<(String, String)>> {
    if cfg!(test) || std::env::var_os("EOE_NO_UPDATE_CHECK").is_some() {
        return None;
    }
    let installed = installed_rev()?;
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        if let Some(remote) = remote_rev() {
            if is_newer(&installed, &remote) {
                let _ = tx.send((installed, remote));
            }
        }
    });
    Some(rx)
}

fn remote_rev() -> Option<String> {
    let repo = std::env::var("EOE_REPO").unwrap_or_else(|_| REPO.to_string());
    let mut child = Command::new("git")
        .args(["ls-remote", &repo, "HEAD"])
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let started = Instant::now();
    loop {
        match child.try_wait().ok()? {
            Some(status) if status.success() => break,
            Some(_) => return None,
            None if started.elapsed() > CHECK_TIMEOUT => {
                let _ = child.kill();
                return None;
            }
            None => std::thread::sleep(Duration::from_millis(100)),
        }
    }
    let mut out = String::new();
    child.stdout.take()?.read_to_string(&mut out).ok()?;
    let sha = out.split_whitespace().next()?;
    (sha.len() >= 7).then(|| sha[..7].to_string())
}

pub fn run_installer() -> std::io::Result<ExitStatus> {
    let url = std::env::var("EOE_INSTALL_URL").unwrap_or_else(|_| INSTALL_URL.to_string());
    Command::new("bash").arg("-c").arg(format!("curl -fsSL '{}' | bash", url)).status()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_different_remote_commit_is_newer_but_a_prefix_match_is_not() {
        assert!(is_newer("40e5f66", "5a1b2c3"));
        assert!(!is_newer("40e5f66", "40e5f66"));
        assert!(!is_newer("40e5f661", "40e5f66"), "a longer local hash of the same commit");
        assert!(!is_newer("40e5f66", "40e5f66a"));
    }

    #[test]
    fn tests_never_start_a_network_check_even_when_installed() {
        let dir = std::env::temp_dir().join(format!("ee-update-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("eoe")).unwrap();
        std::fs::write(dir.join("eoe/installed-rev"), "abc1234\n").unwrap();
        let saved = std::env::var_os("XDG_DATA_HOME");
        std::env::set_var("XDG_DATA_HOME", &dir);
        std::env::set_var("EOE_REPO", dir.join("no-such-repo"));
        std::env::remove_var("EOE_NO_UPDATE_CHECK");
        assert_eq!(installed_rev().as_deref(), Some("abc1234"), "the install record is found");
        assert!(spawn_check().is_none(), "but tests never check the network");
        match saved {
            Some(v) => std::env::set_var("XDG_DATA_HOME", v),
            None => std::env::remove_var("XDG_DATA_HOME"),
        }
        std::env::remove_var("EOE_REPO");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
