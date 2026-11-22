//! Startup hook installation for supported shells.

use std::fs;
use std::path::{Path, PathBuf};

use clap::ValueEnum;
use color_eyre::eyre::eyre;
use color_eyre::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Shell {
    Sharky,
    Bash,
    Zsh,
    Fish,
}

impl Shell {
    pub const fn label(self) -> &'static str {
        match self {
            Shell::Sharky => "Sharky",
            Shell::Bash => "Bash",
            Shell::Zsh => "Zsh",
            Shell::Fish => "Fish",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Created,
    Updated,
    AlreadyPresent,
    Removed,
    NotPresent,
}

const SHARKY_SNIPPET: &str = "# >>> halofetch >>>\n\
    if command -v halofetch >/dev/null 2>&1; then\n\
        wake \"halofetch\"\n\
    fi\n\
    # <<< halofetch <<<\n";
const POSIX_SNIPPET: &str = "# >>> halofetch >>>\n\
    case $- in\n\
        *i*)\n\
            command -v halofetch >/dev/null 2>&1 && halofetch\n\
            ;;\n\
    esac\n\
    # <<< halofetch <<<\n";
const FISH_SNIPPET: &str = "# >>> halofetch >>>\n\
    if status is-interactive; and command -q halofetch\n\
        halofetch\n\
    end\n\
    # <<< halofetch <<<\n";

pub fn snippet(shell: Shell) -> &'static str {
    match shell {
        Shell::Sharky => SHARKY_SNIPPET,
        Shell::Bash | Shell::Zsh => POSIX_SNIPPET,
        Shell::Fish => FISH_SNIPPET,
    }
}

fn markers(shell: Shell) -> (&'static str, &'static str) {
    match shell {
        Shell::Sharky => ("# >>> halofetch >>>", "# <<< halofetch <<<"),
        _ => ("# >>> halofetch >>>", "# <<< halofetch <<<"),
    }
}

pub fn config_path(
    shell: Shell,
    home: &Path,
    xdg_config_home: Option<&Path>,
    sharky_config: Option<&Path>,
    zdotdir: Option<&Path>,
) -> PathBuf {
    match shell {
        Shell::Sharky => {
            let dir = sharky_config
                .map(Path::to_path_buf)
                .or_else(|| xdg_config_home.map(|dir| dir.join("sharky")))
                .unwrap_or_else(|| home.join(".config/sharky"));
            dir.join("config.sharky")
        }
        Shell::Bash => home.join(".bashrc"),
        Shell::Zsh => zdotdir
            .filter(|dir| dir.is_absolute())
            .map(|dir| dir.join(".zshrc"))
            .unwrap_or_else(|| home.join(".zshrc")),
        Shell::Fish => {
            let dir = xdg_config_home
                .filter(|dir| dir.is_absolute())
                .map(|dir| dir.join("fish"))
                .unwrap_or_else(|| home.join(".config/fish"));
            dir.join("config.fish")
        }
    }
}

/// Byte range of the first complete marked block, including both markers.
fn block_span(text: &str, shell: Shell) -> Option<(usize, usize)> {
    let (begin, finish) = markers(shell);
    let mut start = None;
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let code = line.trim_end_matches(['\r', '\n']);
        if code == begin {
            start = Some(offset);
        } else if code == finish {
            if let Some(from) = start {
                return Some((from, offset + line.len()));
            }
        }
        offset += line.len();
    }
    None
}

fn parent_dir(path: &Path) -> &Path {
    path.parent()
        .filter(|dir| !dir.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

fn write_atomic(path: &Path, content: &str) -> Result<()> {
    crate::atomic::write(path, content.as_bytes())
        .map_err(|error| eyre!("cannot write {}: {error}", path.display()))
}

pub fn install(shell: Shell, path: &Path) -> Result<Outcome> {
    let existing = match fs::read_to_string(path) {
        Ok(text) => Some(text),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
        Err(err) => return Err(eyre!("cannot read {}: {err}", path.display())),
    };
    if let Some(text) = &existing {
        if block_span(text, shell).is_some() {
            return Ok(Outcome::AlreadyPresent);
        }
    }

    let created = existing.is_none();
    let mut content = existing.unwrap_or_default();
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    content.push_str(snippet(shell));
    fs::create_dir_all(parent_dir(path))
        .map_err(|err| eyre!("cannot create {}: {err}", parent_dir(path).display()))?;
    write_atomic(path, &content)?;
    Ok(if created {
        Outcome::Created
    } else {
        Outcome::Updated
    })
}

pub fn remove(shell: Shell, path: &Path) -> Result<Outcome> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Outcome::NotPresent),
        Err(err) => return Err(eyre!("cannot read {}: {err}", path.display())),
    };
    let Some((from, to)) = block_span(&text, shell) else {
        return Ok(Outcome::NotPresent);
    };
    let mut content = String::with_capacity(text.len());
    content.push_str(&text[..from]);
    content.push_str(&text[to..]);
    write_atomic(path, &content)?;
    Ok(Outcome::Removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let dir =
                std::env::temp_dir().join(format!("halofetch-init-{}-{name}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Scratch(dir)
        }

        fn path(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn install_creates_a_missing_file_with_the_snippet() {
        let scratch = Scratch::new("create");
        let path = scratch.path("nested/sharky/config.sharky");

        assert_eq!(install(Shell::Sharky, &path).unwrap(), Outcome::Created);
        assert_eq!(fs::read_to_string(&path).unwrap(), snippet(Shell::Sharky));
    }

    #[test]
    fn installing_twice_leaves_the_file_unchanged() {
        let scratch = Scratch::new("twice");
        let path = scratch.path(".zshrc");

        assert_eq!(install(Shell::Zsh, &path).unwrap(), Outcome::Created);
        let first = fs::read_to_string(&path).unwrap();
        assert_eq!(install(Shell::Zsh, &path).unwrap(), Outcome::AlreadyPresent);
        assert_eq!(fs::read_to_string(&path).unwrap(), first);
    }

    #[test]
    fn install_appends_without_rewriting_existing_content() {
        let scratch = Scratch::new("append");
        let path = scratch.path(".bashrc");
        fs::write(&path, "export FOO=1").unwrap();

        assert_eq!(install(Shell::Bash, &path).unwrap(), Outcome::Updated);
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            format!("export FOO=1\n{}", snippet(Shell::Bash))
        );
    }

    #[cfg(unix)]
    #[test]
    fn install_preserves_the_existing_file_mode() {
        use std::os::unix::fs::PermissionsExt;

        let scratch = Scratch::new("permissions");
        let path = scratch.path(".bashrc");
        fs::write(&path, "export FOO=1\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();

        install(Shell::Bash, &path).unwrap();

        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[cfg(unix)]
    #[test]
    fn install_refuses_to_replace_a_symlink() {
        use std::os::unix::fs::symlink;

        let scratch = Scratch::new("symlink");
        let target = scratch.path("dotfile");
        fs::write(&target, "export FOO=1\n").unwrap();
        let link = scratch.path(".bashrc");
        symlink(&target, &link).unwrap();

        let error = install(Shell::Bash, &link).unwrap_err();

        assert!(format!("{error}").contains("symlink"));
        assert!(fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(fs::read_to_string(&target).unwrap(), "export FOO=1\n");
    }

    #[test]
    fn install_detects_a_block_in_a_file_without_a_final_newline() {
        let scratch = Scratch::new("detect");
        let path = scratch.path("config.fish");
        fs::write(&path, snippet(Shell::Fish).trim_end()).unwrap();

        assert_eq!(
            install(Shell::Fish, &path).unwrap(),
            Outcome::AlreadyPresent
        );
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            snippet(Shell::Fish).trim_end()
        );
    }

    #[test]
    fn install_reports_unreadable_files_clearly() {
        let scratch = Scratch::new("unreadable");
        let path = scratch.path("config.sharky");
        fs::create_dir(&path).unwrap();

        let err = install(Shell::Sharky, &path).unwrap_err().to_string();
        assert!(
            err.contains(&format!("cannot read {}", path.display())),
            "{err}"
        );
    }

    #[test]
    fn remove_deletes_the_block_and_keeps_surrounding_lines() {
        let scratch = Scratch::new("remove");
        let path = scratch.path(".bashrc");
        fs::write(&path, format!("before\n{}after\n", snippet(Shell::Bash))).unwrap();

        assert_eq!(remove(Shell::Bash, &path).unwrap(), Outcome::Removed);
        assert_eq!(fs::read_to_string(&path).unwrap(), "before\nafter\n");
        assert_eq!(remove(Shell::Bash, &path).unwrap(), Outcome::NotPresent);
    }

    #[test]
    fn paths_resolve_per_shell() {
        let home = Path::new("/home/test");
        let xdg = std::env::temp_dir().join("halofetch-xdg");
        let xdg = xdg.as_path();
        let sharky = Path::new("/custom/sharky");

        assert_eq!(
            config_path(Shell::Sharky, home, Some(xdg), Some(sharky), None),
            sharky.join("config.sharky")
        );
        assert_eq!(
            config_path(Shell::Sharky, home, Some(xdg), None, None),
            xdg.join("sharky/config.sharky")
        );
        assert_eq!(
            config_path(Shell::Sharky, home, None, None, None),
            home.join(".config/sharky/config.sharky")
        );
        assert_eq!(
            config_path(Shell::Sharky, home, None, Some(Path::new("relative")), None),
            Path::new("relative/config.sharky")
        );
        assert_eq!(
            config_path(Shell::Bash, home, Some(xdg), Some(sharky), None),
            home.join(".bashrc")
        );
        assert_eq!(
            config_path(Shell::Zsh, home, Some(xdg), Some(sharky), None),
            home.join(".zshrc")
        );
        let zdotdir = std::env::temp_dir().join("halofetch-zdotdir");
        assert_eq!(
            config_path(
                Shell::Zsh,
                home,
                Some(xdg),
                Some(sharky),
                Some(zdotdir.as_path())
            ),
            zdotdir.join(".zshrc")
        );
        assert_eq!(
            config_path(Shell::Zsh, home, None, None, Some(Path::new("relative"))),
            home.join(".zshrc")
        );
        assert_eq!(
            config_path(Shell::Fish, home, Some(xdg), None, None),
            xdg.join("fish/config.fish")
        );
        assert_eq!(
            config_path(Shell::Fish, home, Some(Path::new("relative")), None, None),
            home.join(".config/fish/config.fish")
        );
    }

    #[test]
    fn shell_labels_and_commands_are_stable() {
        let all = [Shell::Sharky, Shell::Bash, Shell::Zsh, Shell::Fish];
        let labels: Vec<&str> = all.iter().map(|shell| shell.label()).collect();
        assert_eq!(labels, ["Sharky", "Bash", "Zsh", "Fish"]);

        assert!(snippet(Shell::Sharky).contains("wake \"halofetch\""));
        for shell in [Shell::Bash, Shell::Zsh] {
            assert!(snippet(shell).contains("command -v halofetch >/dev/null 2>&1 && halofetch"));
        }
        assert!(snippet(Shell::Fish).contains("status is-interactive"));
        assert!(snippet(Shell::Fish).contains("command -q halofetch"));
        assert!(snippet(Shell::Sharky).contains("if command -v halofetch"));
        assert!(!snippet(Shell::Sharky).contains("//"));
        assert!(snippet(Shell::Sharky).starts_with("# >>> halofetch >>>\n"));
        assert!(snippet(Shell::Bash).starts_with("# >>> halofetch >>>\n"));
        assert!(snippet(Shell::Fish).ends_with("# <<< halofetch <<<\n"));
    }

    #[cfg(unix)]
    #[test]
    fn sourced_hooks_do_not_run_in_non_interactive_shells() {
        use std::os::unix::fs::PermissionsExt;
        use std::process::Command;

        let scratch = Scratch::new("guards");
        let marker = scratch.path("ran");
        let fake = scratch.path("halofetch");
        fs::write(&fake, "#!/bin/sh\necho ran >> \"$HALO_MARKER\"\n").unwrap();
        fs::set_permissions(&fake, fs::Permissions::from_mode(0o755)).unwrap();
        let path_var = format!(
            "{}:{}",
            scratch.0.display(),
            std::env::var("PATH").unwrap_or_default()
        );

        for (shell, file, snippet) in [
            ("bash", "bashrc", snippet(Shell::Bash)),
            ("fish", "config.fish", snippet(Shell::Fish)),
        ] {
            let config = scratch.path(file);
            fs::write(&config, snippet).unwrap();
            let output = Command::new(shell)
                .args(["-c", &format!("source '{}'", config.display())])
                .env("PATH", &path_var)
                .env("HALO_MARKER", &marker)
                .output();
            let Ok(output) = output else {
                continue;
            };
            assert!(output.status.success(), "{shell}: {output:?}");
            assert!(!marker.exists(), "{shell} ran the fetch non-interactively");
        }
    }

    #[cfg(unix)]
    #[test]
    fn installed_snippets_are_shell_syntax() {
        use std::process::Command;

        let sharky_snippet = snippet(Shell::Sharky);
        if Command::new("sharky").arg("--version").output().is_ok() {
            let scratch = Scratch::new("sharky-syntax");
            let path = scratch.path("config.sharky");
            fs::write(&path, sharky_snippet).unwrap();
            let output = Command::new("sharky")
                .args(["--check", path.to_str().unwrap()])
                .output()
                .unwrap();
            assert!(output.status.success(), "sharky: {output:?}");
        }

        for (program, snippet) in [
            ("bash", snippet(Shell::Bash)),
            ("fish", snippet(Shell::Fish)),
        ] {
            if Command::new(program).arg("--version").output().is_err() {
                continue;
            }
            let flags: &[&str] = if program == "fish" {
                &["--no-execute", "-c", snippet]
            } else {
                &["-n", "-c", snippet]
            };
            let output = Command::new(program).args(flags).output().unwrap();
            assert!(output.status.success(), "{program}: {output:?}");
        }
    }
}
