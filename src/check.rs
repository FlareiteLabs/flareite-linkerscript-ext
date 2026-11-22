//! Checking a script with a real linker: ld.lld (or Rust's rust-lld) or GNU
//! ld links nothing with it, which reads the whole script and evaluates it,
//! and what the linker says becomes problems.
//!
//! - **No objects.** lld links an empty input (the null device) for the
//!   emulation `-m` names, writing the output to standard output (`-o -`),
//!   so nothing is written anywhere. GNU ld can't write to standard output
//!   on Windows, or read and write the same file, so it writes to `NUL.elf`
//!   there (Windows' null device, whatever the extension) and to a pipe
//!   (`/dev/stdout`) elsewhere.
//! - **Symbols from objects.** Symbols the script uses but doesn't define
//!   would come from object files; `--defsym` defines them (as 0x1000, not
//!   0, so alignments and divisions still work).
//! - **What isn't the script's fault** is left out: the entry symbol not
//!   found, ASSERTs (which an empty program may fail), missing input files
//!   and libraries, and failing to write the output.

/// The linker to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Linker {
  Lld,
  RustLld,
  Gnu,
}

impl Linker {
  /// What it's called in messages: the command that runs.
  pub fn command(self) -> &'static str {
    match self {
      Self::Lld => "ld.lld",
      Self::RustLld => "rust-lld",
      Self::Gnu => "ld",
    }
  }

  /// Its `[[programs]]` name (which can't have a dot: lld runs `ld.lld`).
  pub fn program(self) -> &'static str {
    match self {
      Self::Lld => "lld",
      Self::RustLld => "rust-lld",
      Self::Gnu => "ld",
    }
  }
}

/// lld's emulation (`-m`) for an OUTPUT_FORMAT or OUTPUT_ARCH: `Ok(None)` if
/// neither is known, `Err` if it isn't ELF (PE, binary, ...).
pub fn emulation(formats: &[String], arch: Option<&str>) -> Result<Option<&'static str>, String> {
  if let Some(format) = formats.first() {
    let format = format.to_ascii_lowercase();
    let found = match format.as_str() {
      "elf64-x86-64" => Some("elf_x86_64"),
      "elf32-x86-64" => Some("elf32_x86_64"),
      "elf32-i386" => Some("elf_i386"),
      "elf64-littleaarch64" => Some("aarch64elf"),
      "elf64-bigaarch64" => Some("aarch64elfb"),
      "elf32-littlearm" => Some("armelf"),
      "elf32-bigarm" => Some("armelfb"),
      "elf32-littleriscv" => Some("elf32lriscv"),
      "elf64-littleriscv" => Some("elf64lriscv"),
      "elf32-powerpc" => Some("elf32ppc"),
      "elf64-powerpc" => Some("elf64ppc"),
      "elf64-powerpcle" => Some("elf64lppc"),
      "elf32-tradlittlemips" | "elf32-littlemips" => Some("elf32ltsmip"),
      "elf32-tradbigmips" | "elf32-bigmips" => Some("elf32btsmip"),
      "elf64-tradlittlemips" => Some("elf64ltsmip"),
      "elf64-tradbigmips" => Some("elf64btsmip"),
      "elf32-msp430" => Some("msp430elf"),
      "elf64-loongarch" => Some("elf64loongarch"),
      "elf64-sparc" => Some("elf64_sparc"),
      _ if format.starts_with("elf") => None,
      _ => {
        return Err(format!(
          "lld links only ELF programs, and OUTPUT_FORMAT is {format}"
        ));
      }
    };
    if found.is_some() {
      return Ok(found);
    }
  }
  let Some(arch) = arch else {
    return Ok(None);
  };
  Ok(match arch.to_ascii_lowercase().as_str() {
    "i386:x86-64" => Some("elf_x86_64"),
    "i386:x64-32" => Some("elf32_x86_64"),
    "i386" | "i386:i386" => Some("elf_i386"),
    "aarch64" => Some("aarch64elf"),
    "arm" => Some("armelf"),
    "riscv:rv32" => Some("elf32lriscv"),
    "riscv" | "riscv:rv64" => Some("elf64lriscv"),
    "powerpc" | "powerpc:common" => Some("elf32ppc"),
    "powerpc:common64" => Some("elf64ppc"),
    "mips" => Some("elf32btsmip"),
    "msp430" | "msp:430" => Some("msp430elf"),
    "loongarch64" => Some("elf64loongarch"),
    _ => None,
  })
}

/// What to run: the linker's arguments to check `script`.
pub struct Request<'a> {
  pub linker: Linker,
  pub script: &'a str,
  /// `-m`: from the script, the settings, or none.
  pub emulation: Option<&'a str>,
  /// Folders for `-L`: the script's, and the open folder.
  pub search: &'a [String],
  /// Symbols to define with `--defsym`.
  pub defsyms: &'a [String],
  /// The user's `arguments` setting.
  pub extra: &'a [String],
  pub windows: bool,
}

pub fn arguments(request: &Request) -> Vec<String> {
  let mut args = Vec::new();
  if request.linker == Linker::RustLld {
    args.extend(["-flavor".into(), "gnu".into()]);
  }
  if let Some(emulation) = request.emulation {
    args.extend(["-m".into(), emulation.into()]);
  }
  for folder in request.search {
    args.push(format!("-L{folder}"));
  }
  args.extend(["-T".into(), request.script.into()]);
  for name in request.defsyms {
    args.push(format!("--defsym={name}=0x1000"));
  }
  args.extend(request.extra.iter().cloned());
  let (input, output) = match (request.linker, request.windows) {
    (Linker::Gnu, true) => ("NUL", "NUL.elf"),
    (Linker::Gnu, false) => ("/dev/null", "/dev/stdout"),
    (_, true) => ("NUL", "-"),
    (_, false) => ("/dev/null", "-"),
  };
  args.extend([input.into(), "-o".into(), output.into()]);
  args
}

/// A problem the linker reported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
  /// The file as the linker named it: relative to where it ran, or absolute.
  pub file: Option<String>,
  /// One-based; `None` if it named no line.
  pub line: Option<u32>,
  pub warning: bool,
  pub message: String,
}

/// What a linker's output says about the script. `asserts` are its ASSERTs'
/// messages, which are left out.
pub fn reports(output: &str, asserts: &[String]) -> Vec<Report> {
  let mut found: Vec<Report> = Vec::new();
  for line in output.lines() {
    let line = line.trim_end();
    // lld's lines after an error, quoting the script.
    if line.is_empty() || line.starts_with(">>>") || line.starts_with("  ") {
      continue;
    }
    let Some(report) = report(line) else {
      continue;
    };
    if is_noise(&report.message) || asserts.contains(&report.message) {
      continue;
    }
    if !found.contains(&report) {
      found.push(report);
    }
  }
  found
}

fn report(line: &str) -> Option<Report> {
  // lld: `ld.lld: error: script.ld:6: message`
  for (marker, warning) in [(": error: ", false), (": warning: ", true)] {
    if let Some(i) = line.find(marker) {
      let rest = &line[i + marker.len()..];
      let (file, number, message) = located(rest).unwrap_or((None, None, rest));
      return Some(Report {
        file,
        line: number,
        warning,
        message: message.trim().to_owned(),
      });
    }
  }
  // GNU ld: `ld:script.ld:6: warning: message`, or `C:\...\ld.exe: message`.
  let after_program = skip_program(line)?;
  let (file, number, message) = located(after_program).unwrap_or((None, None, after_program));
  let message = message.trim();
  let (warning, message) = match message.strip_prefix("warning: ") {
    Some(message) => (true, message),
    None => (false, message.strip_prefix("error: ").unwrap_or(message)),
  };
  Some(Report {
    file,
    line: number,
    warning,
    message: message.to_owned(),
  })
}

/// The text after a GNU ld line's program name (`ld:`, `C:\...\ld.exe:`).
fn skip_program(line: &str) -> Option<&str> {
  let bytes = line.as_bytes();
  let mut i = 0;
  while let Some(found) = line[i..].find(':') {
    let at = i + found;
    // A drive's colon: `C:\`.
    let drive =
      at == 1 && bytes[0].is_ascii_alphabetic() && matches!(bytes.get(2), Some(b'\\' | b'/'));
    if !drive {
      return Some(&line[at + 1..]);
    }
    i = at + 1;
  }
  None
}

/// `file:line: message` → the file, the line, and the message.
fn located(text: &str) -> Option<(Option<String>, Option<u32>, &str)> {
  let bytes = text.as_bytes();
  let mut i = 0;
  while let Some(found) = text[i..].find(':') {
    let colon = i + found;
    let digits = bytes[colon + 1..]
      .iter()
      .take_while(|c| c.is_ascii_digit())
      .count();
    if digits > 0 && bytes.get(colon + 1 + digits) == Some(&b':') && colon > 0 {
      let file = text[..colon].trim();
      let number: u32 = text[colon + 1..colon + 1 + digits].parse().ok()?;
      let message = &text[colon + 1 + digits + 1..];
      return Some((Some(file.to_owned()), Some(number), message));
    }
    i = colon + 1;
  }
  None
}

/// The names a message quotes: `'.text'`, `` `.text' ``, or `".text"`.
pub fn quoted(message: &str) -> Vec<String> {
  let mut names = Vec::new();
  let mut rest = message;
  while let Some(start) = rest.find(['\'', '`', '"']) {
    let after = &rest[start + 1..];
    let Some(end) = after.find(['\'', '"']) else {
      break;
    };
    names.push(after[..end].to_owned());
    rest = &after[end + 1..];
  }
  names
}

/// Messages that come of linking nothing, not of the script.
fn is_noise(message: &str) -> bool {
  let lower = message.to_ascii_lowercase();
  if lower.contains("linker script") {
    // A missing INCLUDE is the script's.
    return false;
  }
  [
    "cannot find entry symbol",
    "cannot find ",
    "cannot open ",
    "unable to find library",
    "unable to find ",
    "no such file",
    "final link failed",
    "cannot open output file",
    "is the same as output file",
    "failed to write output",
    "no input files",
    "not declared",
  ]
  .iter()
  .any(|noise| lower.contains(noise))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn finds_quoted_names() {
    assert_eq!(
      quoted("section '.big' will not fit in region 'FLASH': overflowed by 4096 bytes"),
      [".big", "FLASH"]
    );
    assert_eq!(quoted("memory region `NOPE' not declared"), ["NOPE"]);
    assert!(quoted("no quotes").is_empty());
  }

  #[test]
  fn finds_the_emulation() {
    let formats = |list: &[&str]| list.iter().map(|f| (*f).to_owned()).collect::<Vec<_>>();
    assert_eq!(
      emulation(&formats(&["elf64-x86-64"]), None),
      Ok(Some("elf_x86_64"))
    );
    assert_eq!(
      emulation(
        &formats(&["elf32-littlearm", "elf32-bigarm", "elf32-littlearm"]),
        None
      ),
      Ok(Some("armelf"))
    );
    assert_eq!(emulation(&[], Some("i386:x86-64")), Ok(Some("elf_x86_64")));
    assert_eq!(emulation(&[], Some("riscv")), Ok(Some("elf64lriscv")));
    assert_eq!(emulation(&[], None), Ok(None));
    assert!(emulation(&formats(&["pei-x86-64"]), None).is_err());
    assert_eq!(
      emulation(&formats(&["elf32-weird"]), Some("arm")),
      Ok(Some("armelf"))
    );
  }

  #[test]
  fn builds_arguments() {
    let args = arguments(&Request {
      linker: Linker::RustLld,
      script: "D:\\os\\kernel\\linker.ld",
      emulation: Some("elf_x86_64"),
      search: &["D:\\os\\kernel".into()],
      defsyms: &["__stack".into()],
      extra: &[],
      windows: true,
    });
    assert_eq!(
      args,
      [
        "-flavor",
        "gnu",
        "-m",
        "elf_x86_64",
        "-LD:\\os\\kernel",
        "-T",
        "D:\\os\\kernel\\linker.ld",
        "--defsym=__stack=0x1000",
        "NUL",
        "-o",
        "-"
      ]
    );
    let gnu = arguments(&Request {
      linker: Linker::Gnu,
      script: "/os/a.ld",
      emulation: None,
      search: &[],
      defsyms: &[],
      extra: &["-z".into(), "max-page-size=0x1000".into()],
      windows: false,
    });
    assert_eq!(
      gnu,
      [
        "-T",
        "/os/a.ld",
        "-z",
        "max-page-size=0x1000",
        "/dev/null",
        "-o",
        "/dev/stdout"
      ]
    );
  }

  #[test]
  fn reads_lld_output() {
    let output = "rust-lld: error: D:\\os\\bad.ld:6: symbol not found: foo\n\
                  rust-lld: error: no stack\n\
                  rust-lld: warning: cannot find entry symbol _start; not setting start address\n\
                  ld.lld: error: syntax.ld:3: unexpected EOF\n\
                  >>> SECTIONS {\n\
                  >>>          ^\n\
                  ld.lld: error: inc.ld:2: cannot find linker script missing.ld\n\
                  ld.lld: error: unable to find library -lc\n";
    let found = reports(output, &["no stack".into()]);
    assert_eq!(
      found,
      [
        Report {
          file: Some("D:\\os\\bad.ld".into()),
          line: Some(6),
          warning: false,
          message: "symbol not found: foo".into(),
        },
        Report {
          file: Some("syntax.ld".into()),
          line: Some(3),
          warning: false,
          message: "unexpected EOF".into(),
        },
        Report {
          file: Some("inc.ld".into()),
          line: Some(2),
          warning: false,
          message: "cannot find linker script missing.ld".into(),
        },
      ]
    );
  }

  #[test]
  fn reads_gnu_ld_output() {
    let output = "C:\\msys64\\ucrt64\\bin\\ld.exe:g.ld:2: warning: memory region `NOPE' not declared\n\
                  ld:D:\\x\\g.ld:2: undefined symbol `foo' referenced in expression\n\
                  ld:syntax.ld:0: syntax error\n\
                  ld: cannot find libc.a: No such file or directory\n\
                  ld: final link failed: Illegal seek\n\
                  ld: section .data LMA [0000000000000400,000000000000043f] overlaps section .text LMA [0000000000000000,00000000000004ff]\n";
    let found = reports(output, &[]);
    let summary: Vec<_> = found
      .iter()
      .map(|r| (r.file.as_deref(), r.line, r.warning, r.message.as_str()))
      .collect();
    assert_eq!(
      summary,
      [
        (
          Some("D:\\x\\g.ld"),
          Some(2),
          false,
          "undefined symbol `foo' referenced in expression"
        ),
        (Some("syntax.ld"), Some(0), false, "syntax error"),
        (
          None,
          None,
          false,
          "section .data LMA [0000000000000400,000000000000043f] overlaps section .text LMA [0000000000000000,00000000000004ff]"
        ),
      ]
    );
  }
}
