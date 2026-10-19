#[cfg(unix)]
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use cvx_core::{Error, IoResultExt, Result};
#[cfg(unix)]
use cvx_index::MODE_EXECUTABLE;
use cvx_index::{MODE_FILE, MODE_SYMLINK, path_to_bytes};

/// Content read from the worktree for a repository-relative path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum WorktreeContent {
    /// The path does not exist or is blocked by a symlink/non-directory.
    Missing,
    /// The path is a directory.
    Directory,
    /// The path is a regular file or symbolic link.
    File { mode: u32, content: Vec<u8> },
}

pub(crate) fn read_content(
    work_tree: &Path,
    relative: &[u8],
    filemode: bool,
) -> Result<WorktreeContent> {
    validate_relative_path(relative)?;
    let components: Vec<&[u8]> = relative.split(|byte| *byte == b'/').collect();

    let mut current = work_tree.to_path_buf();
    for (position, component) in components.iter().enumerate() {
        push_component(&mut current, component)?;
        let metadata = match std::fs::symlink_metadata(&current) {
            Ok(metadata) => metadata,
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                ) =>
            {
                return Ok(WorktreeContent::Missing);
            }
            Err(error) => {
                return Err(Error::io(current.display().to_string(), error));
            }
        };

        if position + 1 < components.len() {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Ok(WorktreeContent::Missing);
            }
            continue;
        }

        if metadata.file_type().is_symlink() {
            let target = std::fs::read_link(&current).path_context(&current)?;
            return Ok(WorktreeContent::File {
                mode: MODE_SYMLINK,
                content: path_to_bytes(&target)?,
            });
        }
        if metadata.is_file() {
            let content = std::fs::read(&current).path_context(&current)?;
            return Ok(WorktreeContent::File {
                mode: worktree_mode(&metadata, filemode),
                content,
            });
        }
        return Ok(WorktreeContent::Directory);
    }

    Ok(WorktreeContent::Missing)
}

pub(crate) fn write_file(
    work_tree: &Path,
    relative: &[u8],
    mode: u32,
    content: &[u8],
) -> Result<()> {
    validate_relative_path(relative)?;
    if mode & 0o170_000 == 0o160_000 {
        return Err(Error::unsupported("checking out submodule entries"));
    }

    let components: Vec<&[u8]> = relative.split(|byte| *byte == b'/').collect();

    let mut current = work_tree.to_path_buf();
    for component in &components[..components.len() - 1] {
        push_component(&mut current, component)?;
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
            Ok(_) => {
                return Err(Error::Message(format!(
                    "cannot write {}: an ancestor is not a directory",
                    current.display()
                )));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                std::fs::create_dir(&current).path_context(&current)?;
            }
            Err(error) => {
                return Err(Error::io(current.display().to_string(), error));
            }
        }
    }

    push_component(&mut current, components[components.len() - 1])?;
    match std::fs::symlink_metadata(&current) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
            return Err(Error::Message(format!(
                "cannot write {}: a directory is in the way",
                current.display()
            )));
        }
        Ok(_) => {
            std::fs::remove_file(&current).path_context(&current)?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(Error::io(current.display().to_string(), error));
        }
    }

    if mode & 0o170_000 == 0o120_000 {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            std::os::unix::fs::symlink(OsStr::from_bytes(content), &current)
                .path_context(&current)?;
            return Ok(());
        }
        #[cfg(not(unix))]
        {
            return Err(Error::unsupported(
                "checking out symbolic links on this platform",
            ));
        }
    }

    let permissions = if mode & 0o111 != 0 { 0o755 } else { 0o644 };
    cvx_core::fs::write_atomic(&current, content, Some(permissions))
}

pub(crate) fn remove_file(work_tree: &Path, relative: &[u8]) -> Result<()> {
    validate_relative_path(relative)?;
    let components: Vec<&[u8]> = relative.split(|byte| *byte == b'/').collect();

    let mut current = work_tree.to_path_buf();
    for component in &components[..components.len() - 1] {
        push_component(&mut current, component)?;
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
            Ok(_) => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                return Err(Error::io(current.display().to_string(), error));
            }
        }
    }

    push_component(&mut current, components[components.len() - 1])?;
    match std::fs::symlink_metadata(&current) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
            return Err(Error::Message(format!(
                "cannot remove {}: a directory is in the way",
                current.display()
            )));
        }
        Ok(_) => {
            std::fs::remove_file(&current).path_context(&current)?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(Error::io(current.display().to_string(), error));
        }
    }

    prune_empty_parents(work_tree, &mut current);
    Ok(())
}

fn validate_relative_path(path: &[u8]) -> Result<()> {
    if path.is_empty() || path[0] == b'/' || path.contains(&0) || path.ends_with(b"/") {
        return Err(Error::InvalidPath {
            path: String::from_utf8_lossy(path).into_owned(),
        });
    }

    for component in path.split(|byte| *byte == b'/') {
        if component.is_empty()
            || component == b"."
            || component == b".."
            || component == b".git"
            || component == b".cvx"
        {
            return Err(Error::InvalidPath {
                path: String::from_utf8_lossy(path).into_owned(),
            });
        }
    }

    Ok(())
}

fn prune_empty_parents(work_tree: &Path, path: &mut PathBuf) {
    while let Some(parent) = path.parent() {
        if parent == work_tree || !parent.starts_with(work_tree) {
            break;
        }
        if std::fs::remove_dir(parent).is_err() {
            break;
        }
        *path = parent.to_path_buf();
    }
}

pub(crate) fn worktree_mode(metadata: &std::fs::Metadata, filemode: bool) -> u32 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if filemode && metadata.permissions().mode() & 0o111 != 0 {
            return MODE_EXECUTABLE;
        }
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        let _ = filemode;
    }
    MODE_FILE
}

fn push_component(path: &mut PathBuf, component: &[u8]) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        path.push(OsStr::from_bytes(component));
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let component = std::str::from_utf8(component).map_err(|_| Error::InvalidPath {
            path: String::from_utf8_lossy(component).into_owned(),
        })?;
        path.push(component);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_and_reads_regular_files() {
        let temp = tempfile::tempdir().expect("tempdir");
        write_file(temp.path(), b"dir/file.txt", MODE_FILE, b"hello\n").expect("write");

        let content = read_content(temp.path(), b"dir/file.txt", true).expect("read");
        assert_eq!(
            content,
            WorktreeContent::File {
                mode: MODE_FILE,
                content: b"hello\n".to_vec()
            }
        );
    }

    #[test]
    fn rejects_control_and_traversal_paths() {
        assert!(write_file(Path::new("/tmp"), b".git/config", MODE_FILE, b"x").is_err());
        assert!(write_file(Path::new("/tmp"), b"../escape", MODE_FILE, b"x").is_err());
        assert!(read_content(Path::new("/tmp"), b".cvx/HEAD", true).is_err());
    }

    #[test]
    fn refuses_to_write_through_a_symlink() {
        #[cfg(unix)]
        {
            let temp = tempfile::tempdir().expect("tempdir");
            std::fs::create_dir(temp.path().join("real")).expect("mkdir");
            std::os::unix::fs::symlink("real", temp.path().join("link")).expect("symlink");
            assert!(write_file(temp.path(), b"link/file", MODE_FILE, b"data").is_err());
        }
    }

    #[test]
    fn removes_files_and_prunes_directories() {
        let temp = tempfile::tempdir().expect("tempdir");
        write_file(temp.path(), b"a/b/c.txt", MODE_FILE, b"data").expect("write");
        remove_file(temp.path(), b"a/b/c.txt").expect("remove");
        assert!(!temp.path().join("a").exists());
    }
}
