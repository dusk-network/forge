use std::{
    env,
    path::{Path, PathBuf},
};

pub fn find_in_path(program: &str) -> Option<PathBuf> {
    let program_path = Path::new(program);
    if program_path.components().count() > 1 {
        return is_executable(program_path).then(|| program_path.to_path_buf());
    }

    let path = env::var_os("PATH")?;
    for dir in env::split_paths(&path) {
        for candidate in program_candidates(program) {
            let full_path = dir.join(candidate);
            if is_executable(&full_path) {
                return Some(full_path);
            }
        }
    }

    None
}

fn program_candidates(program: &str) -> Vec<String> {
    #[cfg(windows)]
    {
        let mut candidates = vec![program.to_string()];
        if Path::new(program).extension().is_none() {
            if let Some(pathext) = env::var_os("PATHEXT") {
                for ext in pathext.to_string_lossy().split(';') {
                    if !ext.is_empty() {
                        candidates.push(format!("{program}{ext}"));
                    }
                }
            }
        }
        candidates
    }

    #[cfg(not(windows))]
    {
        vec![program.to_string()]
    }
}

fn is_executable(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = path
            .metadata()
            .map(|meta| meta.permissions().mode())
            .unwrap_or(0);
        mode & 0o111 != 0
    }

    #[cfg(not(unix))]
    {
        true
    }
}
