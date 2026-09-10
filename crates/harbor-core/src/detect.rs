use std::path::Path;

use crate::app::RuntimeKind;

/// Auto-detect a project's runtime from marker files in its root directory
/// (FR1). Order matters only when a directory carries markers for more than
/// one runtime, which should be rare in practice.
pub fn detect_runtime(dir: &Path) -> Option<RuntimeKind> {
    let has = |name: &str| dir.join(name).is_file();
    let has_glob_ext = |ext: &str| -> bool {
        std::fs::read_dir(dir)
            .map(|entries| {
                entries.filter_map(|e| e.ok()).any(|e| {
                    e.path()
                        .extension()
                        .and_then(|s| s.to_str())
                        .map(|s| s.eq_ignore_ascii_case(ext))
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false)
    };

    if has("Cargo.toml") {
        return Some(RuntimeKind::Rust);
    }
    if has("package.json") {
        return Some(RuntimeKind::Node);
    }
    if has_glob_ext("csproj") || has_glob_ext("sln") {
        return Some(RuntimeKind::DotNet);
    }
    if has("pom.xml") || has("build.gradle") || has("build.gradle.kts") {
        return Some(RuntimeKind::Java);
    }
    if has("requirements.txt") || has("pyproject.toml") || has("setup.py") || has("Pipfile") {
        return Some(RuntimeKind::Python);
    }
    None
}

/// Best-effort default launch command for a detected runtime. Users are
/// expected to override this via `--command` or the config editor (FR2) when
/// the guess doesn't fit the project's actual entrypoint.
pub fn default_command(runtime: RuntimeKind, dir: &Path) -> Vec<String> {
    match runtime {
        RuntimeKind::Python => {
            let venv_python = dir.join(".venv").join("bin").join("python");
            let python = if venv_python.is_file() {
                venv_python.display().to_string()
            } else {
                "python3".to_string()
            };
            for candidate in ["main.py", "app.py", "manage.py"] {
                if dir.join(candidate).is_file() {
                    return vec![python, candidate.to_string()];
                }
            }
            vec![python, "main.py".to_string()]
        }
        RuntimeKind::Node => vec!["npm".to_string(), "start".to_string()],
        RuntimeKind::DotNet => vec!["dotnet".to_string(), "run".to_string()],
        RuntimeKind::Java => {
            if dir.join("pom.xml").is_file() {
                vec!["mvn".to_string(), "spring-boot:run".to_string()]
            } else {
                vec!["./gradlew".to_string(), "bootRun".to_string()]
            }
        }
        RuntimeKind::Rust => vec!["cargo".to_string(), "run".to_string(), "--release".to_string()],
        RuntimeKind::Custom => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_python() {
        let dir = tempdir();
        std::fs::write(dir.join("requirements.txt"), "flask\n").unwrap();
        assert_eq!(detect_runtime(&dir), Some(RuntimeKind::Python));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn detects_node() {
        let dir = tempdir();
        std::fs::write(dir.join("package.json"), "{}").unwrap();
        assert_eq!(detect_runtime(&dir), Some(RuntimeKind::Node));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn detects_nothing_for_empty_dir() {
        let dir = tempdir();
        assert_eq!(detect_runtime(&dir), None);
        std::fs::remove_dir_all(&dir).ok();
    }

    fn tempdir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("harbor-test-{}", std::process::id()))
            .join(format!("{:?}", std::thread::current().id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
