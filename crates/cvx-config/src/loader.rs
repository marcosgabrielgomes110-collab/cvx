use std::env;
use std::path::{Path, PathBuf};

use cvx_core::{Error, Result};

use crate::Config;
use crate::parser::parse_entries;

const MAX_INCLUDE_DEPTH: usize = 16;

impl Config {
    /// Loads a configuration file and follows `include.path` directives.
    ///
    /// Missing files are treated as empty, matching Git's behavior for
    /// optional global and include files.
    pub fn load_file(path: impl AsRef<Path>) -> Result<Self> {
        let mut config = Self::default();
        load_file_into(path.as_ref(), &mut config, 0)?;
        Ok(config)
    }

    /// Loads several files in order, with later values taking precedence.
    pub fn load_files(paths: &[PathBuf]) -> Result<Self> {
        let mut config = Self::default();
        for path in paths {
            load_file_into(path, &mut config, 0)?;
        }
        Ok(config)
    }
}

fn load_file_into(path: &Path, config: &mut Config, depth: usize) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    if depth >= MAX_INCLUDE_DEPTH {
        return Err(Error::config(
            path,
            format!("include nesting exceeds {MAX_INCLUDE_DEPTH} levels"),
        ));
    }

    let input = cvx_core::fs::read_to_string(path)?;
    for entry in parse_entries(&input, path)? {
        if entry.section == "include" && entry.subsection.is_none() && entry.name == "path" {
            if entry.value.trim().is_empty() {
                continue;
            }
            let included = resolve_include_path(&entry.value, path)?;
            load_file_into(&included, config, depth + 1)?;
        } else if entry.section == "includeif" {
            // Conditional includes depend on repository state. They are
            // deliberately ignored until the local configuration layer can
            // evaluate them with full context.
            continue;
        } else {
            config.push(entry);
        }
    }

    Ok(())
}

fn resolve_include_path(value: &str, including_file: &Path) -> Result<PathBuf> {
    let value = value.trim();

    if value == "~" || value.starts_with("~/") || value.starts_with("~\\") {
        let rest = value
            .strip_prefix("~/")
            .or_else(|| value.strip_prefix("~\\"))
            .unwrap_or("");
        let Some(home) = home::home_dir() else {
            return Err(Error::config(
                including_file,
                format!("cannot expand {value}: home directory is unknown"),
            ));
        };
        return Ok(home.join(rest));
    }

    let path = Path::new(value);
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(including_file
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(path))
    }
}

/// Loads the system-wide Git configuration.
pub fn load_system() -> Result<Config> {
    Config::load_file(system_config_path())
}

/// Loads system and user configuration using Git's lookup rules.
///
/// `GIT_CONFIG_NOSYSTEM` and `GIT_CONFIG_GLOBAL` are honored so CVX behaves
/// predictably inside existing tooling and test harnesses. `CVX_*` variants
/// are accepted as well, with the `GIT_*` variables taking precedence.
pub fn load_global() -> Result<Config> {
    let mut config = Config::default();

    if !env_truthy("GIT_CONFIG_NOSYSTEM") && !env_truthy("CVX_CONFIG_NOSYSTEM") {
        load_file_into(&system_config_path(), &mut config, 0)?;
    }

    if let Some(path) =
        env::var_os("GIT_CONFIG_GLOBAL").or_else(|| env::var_os("CVX_CONFIG_GLOBAL"))
    {
        load_file_into(Path::new(&path), &mut config, 0)?;
        return Ok(config);
    }

    let Some(home) = home::home_dir() else {
        return Ok(config);
    };

    let xdg_config_home = env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".config"));
    load_file_into(&xdg_config_home.join("git").join("config"), &mut config, 0)?;
    load_file_into(&home.join(".gitconfig"), &mut config, 0)?;

    Ok(config)
}

fn system_config_path() -> PathBuf {
    #[cfg(windows)]
    {
        if let Some(program_data) = env::var_os("PROGRAMDATA") {
            return PathBuf::from(program_data).join("Git").join("config");
        }
    }

    PathBuf::from("/etc/gitconfig")
}

fn env_truthy(name: &str) -> bool {
    let Some(value) = env::var_os(name) else {
        return false;
    };
    let value = value.to_string_lossy();
    !matches!(
        value.to_ascii_lowercase().as_str(),
        "0" | "false" | "no" | "off"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn follows_relative_includes() {
        let temp = tempfile::tempdir().expect("tempdir");
        let main = temp.path().join("config");
        let included = temp.path().join("extra.conf");

        std::fs::write(&included, "[user]\n name = Included\n").expect("write include");
        std::fs::write(&main, "[include]\n path = extra.conf\n").expect("write main");

        let config = Config::load_file(&main).expect("load");
        assert_eq!(config.get("user.name"), Some("Included"));
    }

    #[test]
    fn ignores_missing_includes() {
        let temp = tempfile::tempdir().expect("tempdir");
        let main = temp.path().join("config");
        std::fs::write(
            &main,
            "[include]\n path = missing.conf\n[user]\n name = Local\n",
        )
        .expect("write");

        let config = Config::load_file(&main).expect("load");
        assert_eq!(config.get("user.name"), Some("Local"));
    }

    #[test]
    fn included_values_keep_file_order() {
        let temp = tempfile::tempdir().expect("tempdir");
        let main = temp.path().join("config");
        let included = temp.path().join("extra.conf");

        std::fs::write(&included, "[user]\n name = Included\n").expect("write include");
        std::fs::write(
            &main,
            "[user]\n name = Before\n[include]\n path = extra.conf\n[user]\n name = After\n",
        )
        .expect("write main");

        let config = Config::load_file(&main).expect("load");
        assert_eq!(config.get("user.name"), Some("After"));
    }
}
