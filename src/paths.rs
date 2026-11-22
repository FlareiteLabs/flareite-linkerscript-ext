//! Paths as the host gives them: absolute, with `\` on Windows.

/// The folder a path is in, and its name.
pub fn split(path: &str) -> (&str, &str) {
  match path.rfind(['/', '\\']) {
    Some(i) => (&path[..i], &path[i + 1..]),
    None => ("", path),
  }
}

pub fn is_windows(path: &str) -> bool {
  path.as_bytes().get(1) == Some(&b':') || path.starts_with("\\\\")
}

pub fn is_absolute(path: &str) -> bool {
  path.starts_with(['/', '\\']) || is_windows(path)
}

/// `relative` inside `folder`, in the folder's style; `..` and `.` resolved.
pub fn join(folder: &str, relative: &str) -> String {
  if is_absolute(relative) {
    return normalize(relative);
  }
  let separator = if is_windows(folder) { '\\' } else { '/' };
  normalize(&format!("{folder}{separator}{relative}"))
}

/// Resolves `.` and `..`, in the path's style.
pub fn normalize(path: &str) -> String {
  let separator = if is_windows(path) { '\\' } else { '/' };
  let leading = if path.starts_with("\\\\") {
    "\\\\"
  } else if path.starts_with(['/', '\\']) {
    if separator == '\\' { "\\" } else { "/" }
  } else {
    ""
  };
  let mut parts: Vec<&str> = Vec::new();
  for part in path[leading.len()..].split(['/', '\\']) {
    match part {
      "" | "." => {}
      ".." => {
        if parts.len() > 1 || (parts.len() == 1 && !is_windows(parts[0])) {
          parts.pop();
        }
      }
      part => parts.push(part),
    }
  }
  format!("{leading}{}", parts.join(&separator.to_string()))
}

pub fn same(a: &str, b: &str) -> bool {
  let normal = |path: &str| normalize(path).replace('\\', "/").to_ascii_lowercase();
  if is_windows(a) {
    normal(a) == normal(b)
  } else {
    normalize(a) == normalize(b)
  }
}

/// `path` relative to `folder`, if it's inside it.
pub fn relative<'a>(folder: &str, path: &'a str) -> Option<&'a str> {
  let folder = folder.trim_end_matches(['/', '\\']);
  if path.len() <= folder.len() || !same(&path[..folder.len()], folder) {
    return None;
  }
  let rest = &path[folder.len()..];
  rest
    .strip_prefix(['/', '\\'])
    .filter(|rest| !rest.is_empty())
}

pub fn is_script(path: &str) -> bool {
  let lower = path.to_ascii_lowercase();
  lower.ends_with(".ld") || lower.ends_with(".lds")
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn joins_and_compares() {
    assert_eq!(
      join("D:\\os\\kernel", "linker.ld"),
      "D:\\os\\kernel\\linker.ld"
    );
    assert_eq!(
      join("D:\\os\\kernel", "../common/mem.ld"),
      "D:\\os\\common\\mem.ld"
    );
    assert_eq!(join("/os/kernel", "./a/b.ld"), "/os/kernel/a/b.ld");
    assert_eq!(join("/os", "/abs/x.ld"), "/abs/x.ld");
    assert!(same("D:\\OS\\a.ld", "d:/os/a.ld"));
    assert!(!same("/os/A.ld", "/os/a.ld"));
    assert_eq!(relative("D:\\os", "D:\\os\\target\\k"), Some("target\\k"));
    assert_eq!(relative("D:\\os", "D:\\other\\k"), None);
    assert!(is_script("a/B.LD") && is_script("x.lds") && !is_script("x.ldx"));
  }
}
