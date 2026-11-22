//! GNU ld linker scripts for Flareite.
//!
//! - **Highlighting, folding, and symbols** come from the Tree-sitter
//!   grammar in `grammar/` and the manifest's queries: Go to Symbol lists
//!   the sections, regions, symbols, and program headers, and Go to
//!   Definition and Find References work within a script.
//! - **Problems as you type** ([`family`]): syntax errors, with ld's
//!   grammar; memory regions, program headers, and sections that aren't
//!   declared, in the script or the scripts it's INCLUDEd with; regions
//!   declared twice; and INCLUDEs that aren't found. Misspelled commands,
//!   functions, and names have a fix.
//! - **The linker's problems when a script is saved** ([`check`]): ld.lld,
//!   rust-lld, or GNU ld links nothing with the script (the one given to
//!   the linker, if the saved one is INCLUDEd), and what it reports goes to
//!   Problems.
//! - **Completion and hover** ([`context`], [`docs`]): what fits where the
//!   cursor is, from ld's commands and functions and the scripts' own
//!   regions, sections, symbols, and program headers.
//! - **The Memory Layout view** ([`layout`], [`elf`]): the program a build
//!   linked, read with `files::read-bytes` (API 2.4): how full each memory
//!   region is, its segments, and its sections with their largest symbols.

mod check;
mod context;
mod docs;
mod elf;
mod family;
mod layout;
mod parse;
mod paths;

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use flareite_extension_api::events::Event;
use flareite_extension_api::language_features::{
  CodeAction, CodeActionRequest, CompletionItem, CompletionKind, CompletionRequest, HoverRequest,
};
use flareite_extension_api::processes::{self, FinishedProcess, ProcessError, ProcessOptions};
use flareite_extension_api::prompts::{self, AnswerValue, PromptAnswer};
use flareite_extension_api::storage::{self, StorageScope};
use flareite_extension_api::types::{
  Activation, CommandContext, ConfigValue, Diagnostic, DocumentId, DocumentInfo, LogLevel,
  MessageLevel, Position, Range, Severity, TextEdit,
};
use flareite_extension_api::views::{
  self, Expansion, ItemIcon, ItemMark, TreeItem, Welcome, WelcomeButton,
};
use flareite_extension_api::workspace::FileChanges;
use flareite_extension_api::{Extension, files, host, register, workbench, workspace};

use check::Linker;
use context::Context;
use family::{Family, Level, Loaded};
use parse::Span;

const LANGUAGE: &str = "linkerscript";
const VIEW: &str = "layout";
const PROGRAM_KEY: &str = "program";
const GO_TO: &str = "flareite.linker-script.goTo";
const CHOOSE: &str = "flareite.linker-script.chooseProgram";

fn log(message: &str) {
  host::log(LogLevel::Info, message);
}

fn setting_text(key: &str) -> String {
  match host::configuration(key) {
    Some(ConfigValue::Text(text)) => text.trim().to_owned(),
    _ => String::new(),
  }
}

fn document_text(id: DocumentId) -> Option<(u64, String)> {
  let info = host::document(id).ok()?;
  let mut lines = Vec::new();
  let mut version = info.version;
  let mut start = 0;
  while start < info.line_count {
    let slice = host::read_lines(id, start, info.line_count).ok()?;
    if slice.lines.is_empty() {
      break;
    }
    version = slice.version;
    start += slice.lines.len() as u32;
    lines.extend(slice.lines);
  }
  Some((version, lines.join("\n")))
}

/// The position of a byte offset in `text`.
fn position(text: &str, offset: usize) -> Position {
  let offset = offset.min(text.len());
  let line_start = text[..offset].rfind('\n').map_or(0, |i| i + 1);
  Position {
    line: text[..offset].matches('\n').count() as u32,
    column: (offset - line_start) as u32,
  }
}

fn range(text: &str, span: &Span) -> Range {
  Range {
    start: position(text, span.start),
    end: position(text, span.end.max(span.start)),
  }
}

/// The byte offset of a position in `text`.
fn offset(text: &str, at: Position) -> usize {
  let mut start = 0;
  for _ in 0..at.line {
    match text[start..].find('\n') {
      Some(i) => start += i + 1,
      None => return text.len(),
    }
  }
  let end = text[start..].find('\n').map_or(text.len(), |i| start + i);
  let mut offset = (start + at.column as usize).min(end);
  while !text.is_char_boundary(offset) {
    offset -= 1;
  }
  offset
}

/// The range of a whole line (1-based), without its indentation.
fn line_range(text: &str, line: u32) -> Range {
  let index = line.saturating_sub(1) as usize;
  let content = text.lines().nth(index).unwrap_or("");
  let indent = content.len() - content.trim_start().len();
  let line = index as u32;
  Range {
    start: Position {
      line,
      column: indent as u32,
    },
    end: Position {
      line,
      column: content.trim_end().len().max(indent + 1) as u32,
    },
  }
}

/// The name at a byte offset in `text`, with its span.
fn word_at(text: &str, at: usize) -> Option<(String, Span)> {
  let bytes = text.as_bytes();
  let mut start = at.min(bytes.len());
  while start > 0 && parse::is_name_char(bytes[start - 1]) {
    start -= 1;
  }
  let mut end = at.min(bytes.len());
  while end < bytes.len() && parse::is_name_char(bytes[end]) {
    end += 1;
  }
  (end > start).then(|| (text[start..end].to_owned(), start..end))
}

fn code_block(text: &str) -> String {
  format!("```ld\n{}\n```", text.trim())
}

struct Open {
  path: String,
}

/// A linker check running.
struct Check {
  /// The script given to the linker.
  root: String,
  linker: Linker,
  cwd: String,
  /// The family's files and texts, for the problems' lines.
  files: Vec<(String, String)>,
  asserts: Vec<String>,
}

#[derive(Default)]
struct LinkerScript {
  documents: BTreeMap<DocumentId, Open>,
  watch: Option<u64>,
  /// The folder's scripts, once listed.
  listed: Option<Vec<String>>,
  /// Files read, by path.
  files: BTreeMap<String, Rc<Loaded>>,
  checks: BTreeMap<u64, Check>,
  /// The latest check of each root script.
  latest: BTreeMap<String, u64>,
  /// Files the linker's problems were published for.
  published: BTreeSet<String>,
  /// The linker `auto` found.
  found_linker: Option<Linker>,
  told_missing: bool,
  /// A program pick showing.
  pick: Option<u64>,
  /// Where to go once a file opens: its path and range.
  reveal: Option<(String, Range)>,
}

impl LinkerScript {
  fn folder() -> Option<String> {
    workspace::folder().map(|folder| folder.path)
  }

  fn watch_folder(&mut self) {
    if let Some(watch) = self.watch.take() {
      workspace::unwatch(watch);
    }
    if let Some(folder) = Self::folder() {
      match workspace::watch(&folder) {
        Ok(watch) => self.watch = Some(watch),
        Err(error) => log(&format!("can't watch the folder for saves: {error:?}")),
      }
    }
  }

  /// The folder's scripts.
  fn listed(&mut self) -> Vec<String> {
    if self.listed.is_none() {
      let found = Self::folder()
        .and_then(|folder| files::list_files(&folder).ok())
        .map(|list| {
          list
            .paths
            .into_iter()
            .filter(|path| paths::is_script(path))
            .take(500)
            .collect()
        })
        .unwrap_or_default();
      self.listed = Some(found);
    }
    self.listed.clone().unwrap_or_default()
  }

  /// A script: an open document's text, or the file's (cached).
  fn load(&mut self, path: &str) -> Option<Rc<Loaded>> {
    if let Some((&id, _)) = self
      .documents
      .iter()
      .find(|(_, open)| paths::same(&open.path, path))
      && let Some((_, text)) = document_text(id)
    {
      return Some(Loaded::new(text));
    }
    let key = paths::normalize(path);
    if let Some(loaded) = self.files.get(&key) {
      return Some(loaded.clone());
    }
    let text = files::read_file(path).ok()?;
    let loaded = Loaded::new(text);
    self.files.insert(key, loaded.clone());
    Some(loaded)
  }

  fn family(&mut self, path: &str) -> Family {
    let listed = self.listed();
    let folder = Self::folder();
    Family::of(path, &listed, folder.as_deref(), &mut |path: &str| {
      self.load(path)
    })
  }

  fn all_scripts(&mut self) -> Family {
    let listed = self.listed();
    let folder = Self::folder();
    Family::all(None, &listed, folder.as_deref(), &mut |path: &str| {
      self.load(path)
    })
  }

  // ----- Problems as you type -----

  fn opened(&mut self, info: &DocumentInfo) {
    let Some(path) = info.path.clone() else {
      return;
    };
    if let Some((want, range)) = self.reveal.clone()
      && paths::same(&want, &path)
    {
      self.reveal = None;
      let _ = workbench::reveal(info.id, range);
    }
    if info.language.as_deref() != Some(LANGUAGE) {
      return;
    }
    self.documents.insert(info.id, Open { path: path.clone() });
    self.lint(info.id);
    if !info.dirty {
      self.check_path(&path);
    }
  }

  fn lint(&mut self, id: DocumentId) {
    let Some(open) = self.documents.get(&id) else {
      return;
    };
    let path = open.path.clone();
    let Some((version, text)) = document_text(id) else {
      return;
    };
    let family = self.family(&path);
    let Some(i) = family.index(&path) else {
      return;
    };
    let diagnostics: Vec<Diagnostic> = family
      .problems(i)
      .into_iter()
      .map(|finding| Diagnostic {
        range: range(&text, &finding.span),
        severity: match finding.level {
          Level::Error => Severity::Error,
          Level::Warning => Severity::Warning,
        },
        message: finding.message,
        code: None,
      })
      .collect();
    if let Err(error) = host::publish_diagnostics(id, version, &diagnostics) {
      log(&format!("couldn't show problems: {error:?}"));
    }
  }

  fn lint_all(&mut self) {
    let ids: Vec<DocumentId> = self.documents.keys().copied().collect();
    for id in ids {
      self.lint(id);
    }
  }

  // ----- The linker's problems -----

  /// The linker the settings choose; `None` for `off`.
  fn linker_setting() -> Option<Vec<Linker>> {
    match setting_text("linker").as_str() {
      "off" => None,
      "ld.lld" => Some(vec![Linker::Lld]),
      "rust-lld" => Some(vec![Linker::RustLld]),
      "ld" => Some(vec![Linker::Gnu]),
      _ => Some(vec![Linker::Lld, Linker::RustLld]),
    }
  }

  /// Checks the scripts given to the linker that `path` is part of.
  fn check_path(&mut self, path: &str) {
    let family = self.family(path);
    if family.members.is_empty() {
      return;
    }
    let Some(candidates) = Self::linker_setting() else {
      self.clear_linker_problems(&family);
      return;
    };
    if workspace::folder().is_some_and(|folder| !folder.trusted) {
      return;
    }
    for root in family.roots() {
      // What the linker reads given this root.
      let linked = family.closure(root);
      // Its own problems are shown already; the linker would repeat them.
      if linked.has_syntax_errors() {
        self.clear_linker_problems(&linked);
        continue;
      }
      let root = linked.roots()[0];
      self.check_root(&linked, root, &candidates);
    }
  }

  fn clear_linker_problems(&mut self, family: &Family) {
    for member in &family.members {
      if self.published.remove(&paths::normalize(&member.path)) {
        let _ = host::publish_file_diagnostics(&member.path, &[]);
      }
    }
  }

  fn check_root(&mut self, family: &Family, root: usize, candidates: &[Linker]) {
    let member = &family.members[root];
    let script = &member.loaded.script;
    let (cwd, _) = paths::split(&member.path);
    let cwd = cwd.to_owned();
    let windows = paths::is_windows(&member.path);
    let emulation_setting = setting_text("emulation");
    let from_script = check::emulation(&script.formats, script.arch.as_deref());
    let mut search = vec![cwd.clone()];
    if let Some(folder) = Self::folder()
      && !paths::same(&folder, &cwd)
    {
      search.push(folder);
    }
    let defsyms = family.undefined_symbols();
    let extra: Vec<String> = setting_text("arguments")
      .split_whitespace()
      .map(str::to_owned)
      .collect();
    let order: Vec<Linker> = match self.found_linker {
      Some(found) if candidates.contains(&found) => vec![found],
      _ => candidates.to_vec(),
    };
    let mut why = String::new();
    for linker in order {
      let emulation = if !emulation_setting.is_empty() {
        Some(emulation_setting.as_str())
      } else if linker == Linker::Gnu {
        // GNU ld has the emulations it was built for; use its default.
        None
      } else {
        match &from_script {
          Ok(found) => Some(found.unwrap_or("elf_x86_64")),
          Err(reason) => {
            log(&format!("not checking {}: {reason}", member.path));
            self.clear_linker_problems(family);
            return;
          }
        }
      };
      let args = check::arguments(&check::Request {
        linker,
        script: &member.path,
        emulation,
        search: &search,
        defsyms: &defsyms,
        extra: &extra,
        windows,
      });
      let options = ProcessOptions {
        cwd: Some(cwd.clone()),
        env: Vec::new(),
        stdin: None,
        timeout_ms: Some(20_000),
      };
      match processes::spawn(linker.program(), &args, &options) {
        Ok(process) => {
          if candidates.len() > 1 {
            self.found_linker = Some(linker);
          }
          self.latest.insert(paths::normalize(&member.path), process);
          self.checks.insert(
            process,
            Check {
              root: member.path.clone(),
              linker,
              cwd,
              files: family
                .members
                .iter()
                .map(|member| (member.path.clone(), member.loaded.text.clone()))
                .collect(),
              asserts: family.asserts(),
            },
          );
          return;
        }
        Err(ProcessError::NotFound(reason)) => {
          why = reason;
        }
        Err(ProcessError::Untrusted) => return,
        Err(error) => {
          log(&format!("{} couldn't be run: {error:?}", linker.program()));
          return;
        }
      }
    }
    if !self.told_missing {
      self.told_missing = true;
      host::show_message(
        MessageLevel::Warning,
        &format!(
          "No linker was found to check linker scripts ({why}). Install LLVM's lld (ld.lld) or Rust (rust-lld), or choose GNU ld with the linker setting; or turn the check off with linker = \"off\". To use one that isn't on the PATH: [extensions.programs] \"flareite.linker-script.lld\" = \"C:/path/to/ld.lld.exe\"."
        ),
      );
    }
  }

  fn finished(&mut self, finished: FinishedProcess) {
    let Some(check) = self.checks.remove(&finished.process) else {
      return;
    };
    if self.latest.get(&paths::normalize(&check.root)) != Some(&finished.process) {
      return;
    }
    let output = match finished.outcome {
      Ok(output) => output,
      Err(error) => {
        log(&format!("{} failed: {error:?}", check.linker.program()));
        return;
      }
    };
    let mut text = String::from_utf8_lossy(&output.stderr).into_owned();
    // GNU ld on a pipe writes the program to standard output: only lld's
    // `-o -` does, and it's binary either way, so it's left out.
    if output.stdout.len() < 64 * 1024 && output.stdout.is_ascii() {
      text.push('\n');
      text.push_str(&String::from_utf8_lossy(&output.stdout));
    }
    let reports = check::reports(&text, &check.asserts);
    let mut by_file: BTreeMap<String, Vec<Diagnostic>> = check
      .files
      .iter()
      .map(|(path, _)| (path.clone(), Vec::new()))
      .collect();
    let name = check.linker.command();
    // Where each file defines its output sections, for reports that name
    // a section but no line.
    let sections: Vec<(String, String, Span)> = check
      .files
      .iter()
      .flat_map(|(path, text)| {
        parse::parse(text)
          .sections
          .into_iter()
          .map(move |section| (section.name.text, path.clone(), section.name.span))
      })
      .collect();
    for report in reports {
      if report.line.is_none() {
        let named = check::quoted(&report.message);
        if let Some((_, path, span)) = sections
          .iter()
          .find(|(section, ..)| named.iter().any(|name| name == section))
        {
          let text = check
            .files
            .iter()
            .find(|(p, _)| p == path)
            .map_or("", |(_, text)| text.as_str());
          by_file.entry(path.clone()).or_default().push(Diagnostic {
            range: range(text, span),
            severity: if report.warning {
              Severity::Warning
            } else {
              Severity::Error
            },
            message: format!("{name}: {}", report.message),
            code: Some("linker".into()),
          });
          continue;
        }
        // A section the scripts don't place (one the linker made, placed
        // after one that overflowed): not the script's to fix.
        if report.message.starts_with("section ") {
          continue;
        }
      }
      let file = report
        .file
        .as_deref()
        .map(|file| paths::join(&check.cwd, file))
        .and_then(|file| {
          check
            .files
            .iter()
            .find(|(path, _)| paths::same(path, &file))
            .map(|(path, _)| path.clone())
        })
        .unwrap_or_else(|| check.root.clone());
      let text = check
        .files
        .iter()
        .find(|(path, _)| *path == file)
        .map_or("", |(_, text)| text.as_str());
      let line = match report.line {
        // GNU ld says line 0 at the end of the file.
        Some(0) => text.lines().count().max(1) as u32,
        Some(line) => line,
        None => 1,
      };
      by_file.entry(file).or_default().push(Diagnostic {
        range: line_range(text, line),
        severity: if report.warning {
          Severity::Warning
        } else {
          Severity::Error
        },
        message: format!("{name}: {}", report.message),
        code: Some("linker".into()),
      });
    }
    for (path, diagnostics) in by_file {
      let key = paths::normalize(&path);
      if diagnostics.is_empty() && !self.published.contains(&key) {
        continue;
      }
      if diagnostics.is_empty() {
        self.published.remove(&key);
      } else {
        self.published.insert(key);
      }
      if let Err(error) = host::publish_file_diagnostics(&path, &diagnostics) {
        log(&format!("couldn't show the linker's problems: {error:?}"));
      }
    }
  }

  fn files_changed(&mut self, changes: &FileChanges) {
    let scripts: Vec<String> = changes
      .paths
      .iter()
      .filter(|path| paths::is_script(path))
      .cloned()
      .collect();
    if changes.overflow {
      self.files.clear();
      self.listed = None;
    }
    for path in &scripts {
      self.files.remove(&paths::normalize(path));
    }
    // A script added or removed.
    if !scripts.is_empty() {
      let known = self.listed.clone().unwrap_or_default();
      if scripts
        .iter()
        .any(|path| !known.iter().any(|k| paths::same(k, path)) || files::read_file(path).is_err())
      {
        self.listed = None;
      }
    }
    if changes.overflow || !scripts.is_empty() {
      self.lint_all();
      // Saved: check with the linker.
      let to_check: Vec<String> = if changes.overflow {
        self
          .documents
          .values()
          .map(|open| open.path.clone())
          .collect()
      } else {
        scripts
      };
      let mut checked: BTreeSet<String> = BTreeSet::new();
      for path in to_check {
        if checked.insert(paths::normalize(&path)) {
          self.check_path(&path);
        }
      }
    }
    let program = self.program();
    if changes.overflow
      || !changes.paths.is_empty()
        && changes.paths.iter().any(|path| {
          paths::is_script(path)
            || program
              .as_deref()
              .is_some_and(|program| paths::same(program, path))
        })
    {
      self.refresh_layout();
    }
  }

  // ----- The Memory Layout view -----

  /// The program the view shows: the `program` setting, or the one chosen.
  fn program(&self) -> Option<String> {
    let folder = Self::folder()?;
    let setting = setting_text(PROGRAM_KEY);
    let chosen = if setting.is_empty() {
      storage::get(StorageScope::Workspace, PROGRAM_KEY)?
    } else {
      setting
    };
    Some(paths::join(&folder, &chosen))
  }

  fn welcome(text: &str) {
    let welcome = Welcome {
      text: text.into(),
      buttons: vec![WelcomeButton {
        label: "Choose Program…".into(),
        command: CHOOSE.into(),
        argument: None,
      }],
    };
    let _ = views::set_items(VIEW, &[]);
    let _ = views::set_welcome(VIEW, Some(&welcome));
  }

  fn refresh_layout(&mut self) {
    let Some(folder) = Self::folder() else {
      let _ = views::set_items(VIEW, &[]);
      let _ = views::set_welcome(
        VIEW,
        Some(&Welcome {
          text: "Open a folder to see the memory layout of a program you build in it.".into(),
          buttons: Vec::new(),
        }),
      );
      return;
    };
    let Some(program) = self.program() else {
      Self::welcome(
        "Choose the program your build links (an ELF file, such as target/<target>/debug/kernel or build/firmware.elf) to see how full each memory region is, and what's in its segments and sections.",
      );
      return;
    };
    let mut read = |offset: u64, length: u32| -> Result<Vec<u8>, String> {
      files::read_bytes(&program, offset, length)
        .map(|slice| slice.bytes)
        .map_err(|error| match error {
          files::FileError::NotFound => "it isn't there (build it, or choose another)".into(),
          files::FileError::OutsideFolder => "it isn't in the open folder".into(),
          files::FileError::PermissionDenied(_) => "the extension can't read files".into(),
          other => format!("{other:?}"),
        })
    };
    let elf = match elf::parse(&mut read) {
      Ok(elf) => elf,
      Err(why) => {
        let shown = paths::relative(&folder, &program).unwrap_or(&program);
        Self::welcome(&format!("{shown} can't be shown: {why}."));
        return;
      }
    };
    let scripts = self.all_scripts();
    let mut regions: Vec<layout::RegionSpan> = Vec::new();
    for (_, script) in scripts.scripts() {
      for region in &script.regions {
        if regions.iter().any(|known| known.name == region.name.text) {
          continue;
        }
        if let Some((origin, length)) = scripts.region_value(&region.name.text) {
          regions.push(layout::RegionSpan {
            name: region.name.text.clone(),
            origin,
            length,
            attributes: region.attributes.clone(),
          });
        }
      }
    }
    let defined: BTreeSet<String> = scripts
      .symbols()
      .into_iter()
      .map(|(_, symbol)| symbol.name.text.clone())
      .collect();
    let shown = paths::relative(&folder, &program)
      .unwrap_or(&program)
      .to_owned();
    let items: Vec<TreeItem> = layout::rows(&elf, &program, &shown, &regions)
      .into_iter()
      .map(|row| {
        let (command, argument) = match &row.action {
          Some(layout::Action::ChooseProgram) => (Some(CHOOSE.to_owned()), None),
          Some(layout::Action::Region(name)) => {
            (Some(GO_TO.to_owned()), Some(format!("region\n{name}")))
          }
          Some(layout::Action::Section(name)) => {
            (Some(GO_TO.to_owned()), Some(format!("section\n{name}")))
          }
          Some(layout::Action::Symbol(name)) if defined.contains(name) => {
            (Some(GO_TO.to_owned()), Some(format!("symbol\n{name}")))
          }
          _ => (None, None),
        };
        TreeItem {
          id: row.id,
          parent: row.parent,
          label: row.label,
          description: row.description,
          tooltip: row.tooltip,
          icon: match row.icon {
            layout::Icon::None => ItemIcon::None,
            layout::Icon::Named(name) => ItemIcon::Named(name.into()),
            layout::Icon::File(path) => ItemIcon::File(path),
          },
          mark: (row.color.is_some() || row.badge.is_some()).then(|| ItemMark {
            badge: row.badge,
            color: row.color.map(str::to_owned),
            strikethrough: false,
            faded: false,
          }),
          context: None,
          expansion: match row.expanded {
            Some(true) => Expansion::Expanded,
            Some(false) => Expansion::Collapsed,
            None => Expansion::None,
          },
          command,
          argument,
        }
      })
      .collect();
    let _ = views::set_welcome(VIEW, None);
    if let Err(error) = views::set_items(VIEW, &items) {
      log(&format!("couldn't show the layout: {error:?}"));
    }
  }

  fn choose_program(&mut self) -> Result<(), String> {
    let prompt = prompts::pick_file("Choose the program your build links (an ELF file)")
      .map_err(|error| format!("{error:?}"))?;
    self.pick = Some(prompt);
    Ok(())
  }

  fn answered(&mut self, answer: PromptAnswer) {
    if self.pick != Some(answer.prompt) {
      return;
    }
    self.pick = None;
    let AnswerValue::Text(path) = answer.value else {
      return;
    };
    let Some(folder) = Self::folder() else {
      return;
    };
    let Some(relative) = paths::relative(&folder, &path) else {
      host::show_message(
        MessageLevel::Warning,
        "Choose a program in the open folder: the extension reads only files there.",
      );
      return;
    };
    if let Err(error) = storage::set(StorageScope::Workspace, PROGRAM_KEY, Some(relative)) {
      log(&format!("couldn't remember the program: {error:?}"));
    }
    if !setting_text(PROGRAM_KEY).is_empty() {
      host::show_message(
        MessageLevel::Info,
        "The program setting chooses the program shown; clear it to show the one you chose.",
      );
    }
    self.refresh_layout();
  }

  /// Goes to where a script defines a region, section, or symbol.
  fn go_to(&mut self, argument: &str) -> Result<(), String> {
    let (kind, name) = argument.split_once('\n').ok_or("nothing to go to")?;
    let scripts = self.all_scripts();
    let found = scripts.scripts().find_map(|(i, script)| {
      let span = match kind {
        "region" => script
          .regions
          .iter()
          .find(|region| region.name.text == name)
          .map(|region| region.name.span.clone())
          .or_else(|| {
            script
              .aliases
              .iter()
              .find(|alias| alias.name.text == name)
              .map(|alias| alias.name.span.clone())
          }),
        "section" => script
          .sections
          .iter()
          .find(|section| section.name.text == name)
          .map(|section| section.name.span.clone()),
        _ => script
          .symbols
          .iter()
          .find(|symbol| symbol.name.text == name)
          .map(|symbol| symbol.name.span.clone()),
      }?;
      Some((i, span))
    });
    let Some((i, span)) = found else {
      host::show_message(
        MessageLevel::Info,
        &format!("No linker script in the open folder defines {name}."),
      );
      return Ok(());
    };
    let member = &scripts.members[i];
    let target = range(&member.loaded.text, &span);
    let open = self
      .documents
      .iter()
      .find(|(_, open)| paths::same(&open.path, &member.path))
      .map(|(&id, _)| id);
    workspace::open_file(&member.path).map_err(|error| format!("{error:?}"))?;
    match open {
      Some(id) => {
        let _ = workbench::reveal(id, target);
      }
      None => self.reveal = Some((member.path.clone(), target)),
    }
    Ok(())
  }

  // ----- Completion and hover -----

  fn names(&mut self, id: DocumentId) -> Option<(String, Family)> {
    let path = self.documents.get(&id)?.path.clone();
    let family = self.family(&path);
    Some((path, family))
  }
}

fn item(
  label: &str,
  kind: CompletionKind,
  detail: Option<String>,
  start: Position,
  insert: String,
  sort: &str,
) -> CompletionItem {
  CompletionItem {
    label: label.to_owned(),
    kind,
    detail,
    start,
    insert,
    filter: Some(label.to_owned()),
    sort: Some(format!("{sort}{label}")),
    deprecated: false,
  }
}

/// A command or function as it's inserted: `NAME(` if it takes arguments.
fn keyword_item(name: &str, start: Position, sort: &str) -> CompletionItem {
  let doc = docs::find(name);
  let takes_arguments = doc.is_some_and(|doc| doc.usage.starts_with(&format!("{name}(")));
  let kind = if parse::FUNCTIONS.contains(&name) && !parse::TOP_COMMANDS.contains(&name) {
    CompletionKind::Function
  } else {
    CompletionKind::Keyword
  };
  item(
    name,
    kind,
    doc.map(|doc| doc.usage.to_owned()),
    start,
    if takes_arguments {
      format!("{name}(")
    } else {
      name.to_owned()
    },
    sort,
  )
}

impl Extension for LinkerScript {
  fn activate(_: Activation) -> Result<Self, String> {
    let mut extension = LinkerScript::default();
    extension.watch_folder();
    extension.refresh_layout();
    Ok(extension)
  }

  fn run_command(&mut self, command: &str, context: CommandContext) -> Result<(), String> {
    match command {
      CHOOSE => self.choose_program(),
      "flareite.linker-script.refreshLayout" => {
        self.files.clear();
        self.listed = None;
        self.refresh_layout();
        Ok(())
      }
      "flareite.linker-script.check" => {
        self.files.clear();
        self.listed = None;
        let path = workbench::active_file()
          .filter(|path| paths::is_script(path))
          .or_else(|| self.documents.values().next().map(|open| open.path.clone()))
          .ok_or("Open a linker script to check it.")?;
        if Self::linker_setting().is_none() {
          host::show_message(
            MessageLevel::Info,
            "The linker check is off: set linker to auto, ld.lld, rust-lld, or ld.",
          );
        }
        self.check_path(&path);
        Ok(())
      }
      GO_TO => self.go_to(context.argument.as_deref().unwrap_or_default()),
      other => Err(format!("unknown command {other}")),
    }
  }

  fn handle_event(&mut self, event: Event) -> Result<(), String> {
    match event {
      Event::DocumentOpened(info) => self.opened(&info),
      Event::DocumentChanged(change) => {
        if self.documents.contains_key(&change.document) {
          // Other open scripts may use what this one declares.
          self.lint_all();
        }
      }
      Event::DocumentClosed(id) => {
        self.documents.remove(&id);
      }
      Event::FilesChanged(changes) if Some(changes.watch) == self.watch => {
        self.files_changed(&changes);
      }
      Event::FolderChanged => {
        self.files.clear();
        self.listed = None;
        self.found_linker = None;
        self.watch_folder();
        self.refresh_layout();
      }
      Event::ConfigurationChanged => {
        self.found_linker = None;
        let paths: Vec<String> = self
          .documents
          .values()
          .map(|open| open.path.clone())
          .collect();
        for path in paths {
          self.check_path(&path);
        }
        self.refresh_layout();
      }
      Event::ProcessFinished(finished) => self.finished(finished),
      Event::PromptAnswered(answer) => self.answered(answer),
      _ => {}
    }
    Ok(())
  }

  fn provide_completions(
    &mut self,
    request: CompletionRequest,
  ) -> Result<Vec<CompletionItem>, String> {
    let Some((_, text)) = document_text(request.document) else {
      return Ok(Vec::new());
    };
    let at = offset(&text, request.position);
    let (found, start) = context::at(&text, at);
    let start = position(&text, start);
    let Some((path, family)) = self.names(request.document) else {
      return Ok(Vec::new());
    };
    let mut items = Vec::new();
    let keywords = |places: &[docs::Place], items: &mut Vec<CompletionItem>| {
      for doc in docs::DOCS {
        if doc.places.iter().any(|place| places.contains(place)) {
          items.push(keyword_item(doc.name, start, "1"));
        }
      }
    };
    let symbols = |items: &mut Vec<CompletionItem>| {
      let mut seen = BTreeSet::new();
      for (_, symbol) in family.symbols() {
        if seen.insert(symbol.name.text.clone()) {
          items.push(item(
            &symbol.name.text,
            CompletionKind::Variable,
            Some(symbol.source.clone()),
            start,
            symbol.name.text.clone(),
            "0",
          ));
        }
      }
    };
    match found {
      Context::Nothing | Context::MemoryName => {}
      Context::Top => keywords(&[docs::Place::Top], &mut items),
      Context::Sections => {
        keywords(&[docs::Place::Sections], &mut items);
        for name in [".text", ".rodata", ".data", ".bss", "/DISCARD/"] {
          items.push(item(
            name,
            CompletionKind::Module,
            Some("output section".into()),
            start,
            name.into(),
            "2",
          ));
        }
      }
      Context::Body => {
        keywords(&[docs::Place::Body], &mut items);
        items.push(item(
          ". = ALIGN(",
          CompletionKind::Snippet,
          Some("align the location counter".into()),
          start,
          ". = ALIGN(".into(),
          "0",
        ));
        symbols(&mut items);
      }
      Context::Attribute => {
        keywords(&[docs::Place::Attribute], &mut items);
      }
      Context::SectionType => keywords(&[docs::Place::Type], &mut items),
      Context::MemoryOrigin => {
        items.push(item(
          "ORIGIN = ",
          CompletionKind::Keyword,
          Some("where the region starts".into()),
          start,
          "ORIGIN = ".into(),
          "0",
        ));
      }
      Context::MemoryLength => {
        items.push(item(
          "LENGTH = ",
          CompletionKind::Keyword,
          Some("the region's size".into()),
          start,
          "LENGTH = ".into(),
          "0",
        ));
      }
      Context::Region => {
        for name in family.region_names() {
          let detail = family.region_value(&name).map(|(origin, length)| {
            format!("{} · {}", layout::address(origin, 32), layout::size(length))
          });
          items.push(item(
            &name,
            CompletionKind::Struct,
            detail,
            start,
            name.clone(),
            "0",
          ));
        }
      }
      Context::Section => {
        for name in family.section_names() {
          items.push(item(
            &name,
            CompletionKind::Module,
            Some("output section".into()),
            start,
            name.clone(),
            "0",
          ));
        }
      }
      Context::Phdr => {
        for name in family.phdr_names() {
          items.push(item(
            &name,
            CompletionKind::Enum,
            Some("program header".into()),
            start,
            name.clone(),
            "0",
          ));
        }
      }
      Context::PhdrType => {
        for (name, text) in docs::PHDR_TYPES {
          items.push(item(
            name,
            CompletionKind::Constant,
            Some((*text).into()),
            start,
            (*name).into(),
            "0",
          ));
        }
      }
      Context::PhdrAttribute => {
        for name in ["FILEHDR", "PHDRS", "AT", "FLAGS"] {
          items.push(keyword_item(name, start, "0"));
        }
      }
      Context::Symbol => symbols(&mut items),
      Context::Expression => {
        keywords(&[docs::Place::Function], &mut items);
        symbols(&mut items);
        items.push(item(
          "SIZEOF_HEADERS",
          CompletionKind::Constant,
          Some("the size of the output's headers".into()),
          start,
          "SIZEOF_HEADERS".into(),
          "2",
        ));
      }
      Context::Constant => {
        for name in ["MAXPAGESIZE", "COMMONPAGESIZE"] {
          let doc = docs::find(name).map(|doc| doc.text.to_owned());
          items.push(item(
            name,
            CompletionKind::Constant,
            doc,
            start,
            name.into(),
            "0",
          ));
        }
      }
      Context::InputSection => {
        for (name, text) in docs::INPUT_SECTIONS {
          items.push(item(
            name,
            CompletionKind::Module,
            Some((*text).into()),
            start,
            (*name).into(),
            "0",
          ));
        }
        for name in [
          "SORT",
          "SORT_BY_NAME",
          "SORT_BY_ALIGNMENT",
          "SORT_BY_INIT_PRIORITY",
          "SORT_NONE",
          "REVERSE",
          "EXCLUDE_FILE",
        ] {
          items.push(keyword_item(name, start, "1"));
        }
      }
      Context::File => {
        // Other scripts, relative to this one's folder.
        let (dir, _) = paths::split(&path);
        let dir = dir.to_owned();
        for script in self.listed() {
          if paths::same(&script, &path) {
            continue;
          }
          if let Some(relative) = paths::relative(&dir, &script) {
            let relative = relative.replace('\\', "/");
            items.push(item(
              &relative,
              CompletionKind::File,
              Some("linker script".into()),
              start,
              relative.clone(),
              "0",
            ));
          }
        }
      }
    }
    Ok(items)
  }

  fn provide_hover(&mut self, request: HoverRequest) -> Result<Option<String>, String> {
    let Some((_, text)) = document_text(request.document) else {
      return Ok(None);
    };
    let at = offset(&text, request.position);
    let Some((word, span)) = word_at(&text, at) else {
      return Ok(None);
    };
    if context::in_comment_or_string(&text, span.start) {
      return Ok(None);
    }
    let Some((path, family)) = self.names(request.document) else {
      return Ok(None);
    };
    let me = family.index(&path);
    let called = text[span.end..].trim_start().starts_with('(');
    let in_memory = text[..span.start].rfind("MEMORY").is_some_and(|i| {
      let after = &text[i..span.start];
      after.matches('{').count() > after.matches('}').count()
    });

    // A memory region.
    if let Some((i, region)) = family.scripts().find_map(|(i, script)| {
      script
        .regions
        .iter()
        .find(|r| r.name.text == word)
        .map(|r| (i, r))
    }) && !called
    {
      let mut hover = format!("**{word}**");
      if !region.attributes.is_empty() {
        hover.push_str(&format!(" ({})", region.attributes));
      }
      hover.push_str(" · memory region");
      if let Some((origin, length)) = family.region_value(&word) {
        hover.push_str(&format!(
          "\n\n`{}` – `{}` · {}",
          layout::address(origin, 32),
          layout::address(origin.saturating_add(length).saturating_sub(1), 32),
          layout::size(length)
        ));
      }
      let from = me.unwrap_or(i);
      hover.push_str(&format!(
        "\n\n*Declared {}*",
        family.place(i, region.name.span.start, from)
      ));
      return Ok(Some(hover));
    }
    if let Some((i, alias)) = family.scripts().find_map(|(i, script)| {
      script
        .aliases
        .iter()
        .find(|a| a.name.text == word)
        .map(|a| (i, a))
    }) {
      let from = me.unwrap_or(i);
      return Ok(Some(format!(
        "**{word}** · another name for the memory region **{}**\n\n*Declared {}*",
        alias.region.text,
        family.place(i, alias.name.span.start, from)
      )));
    }
    // An output section.
    let sections: Vec<_> = family
      .scripts()
      .flat_map(|(i, script)| {
        script
          .sections
          .iter()
          .filter(|s| s.name.text == word)
          .map(move |s| (i, s))
      })
      .collect();
    if let Some((i, section)) = sections.first() {
      let mut hover = code_block(&section.header);
      let mut facts = Vec::new();
      if let Some(region) = &section.region {
        facts.push(format!("runs in **{}**", region.text));
      }
      if let Some(region) = &section.load_region {
        facts.push(format!("is loaded in **{}**", region.text));
      }
      if !section.phdrs.is_empty() {
        let names: Vec<_> = section.phdrs.iter().map(|p| p.text.as_str()).collect();
        facts.push(format!("goes in segment {}", names.join(", ")));
      }
      hover.push_str("\n\nOutput section");
      if !facts.is_empty() {
        hover.push_str(": ");
        hover.push_str(&facts.join(", "));
      }
      let from = me.unwrap_or(*i);
      hover.push_str(&format!(
        "\n\n*Defined {}*",
        family.place(*i, section.name.span.start, from)
      ));
      if sections.len() > 1 {
        hover.push_str(&format!(" (and {} more times)", sections.len() - 1));
      }
      return Ok(Some(hover));
    }
    // ld's own words.
    if let Some(doc) = if in_memory {
      docs::memory_word(&word)
    } else {
      None
    }
    .or_else(|| docs::find(&word))
    {
      return Ok(Some(format!("{}\n\n{}", code_block(doc.usage), doc.text)));
    }
    // A symbol the scripts define.
    let symbols: Vec<_> = family
      .symbols()
      .into_iter()
      .filter(|(_, symbol)| symbol.name.text == word)
      .collect();
    if let Some((i, first)) = symbols.first() {
      let sources: Vec<&str> = symbols
        .iter()
        .take(4)
        .map(|(_, s)| s.source.as_str())
        .collect();
      let mut hover = code_block(&sources.join("\n"));
      let mut facts = Vec::new();
      match first.kind {
        parse::SymbolKind::Provided => {
          facts.push("provided (only if the program uses it and doesn't define it)".to_owned())
        }
        parse::SymbolKind::Hidden => facts.push("hidden".to_owned()),
        parse::SymbolKind::Assigned => {}
      }
      if let Some(section) = &first.section {
        facts.push(format!("in {section}"));
      }
      if let Some(value) = family.symbol_value(&word) {
        facts.push(format!("= `0x{value:x}` ({value})"));
      }
      hover.push_str("\n\nSymbol");
      if !facts.is_empty() {
        hover.push_str(", ");
        hover.push_str(&facts.join(", "));
      }
      let from = me.unwrap_or(*i);
      hover.push_str(&format!(
        "\n\n*Defined {}*",
        family.place(*i, first.name.span.start, from)
      ));
      return Ok(Some(hover));
    }
    // A program header.
    if let Some((i, phdr)) = family.scripts().find_map(|(i, script)| {
      script
        .phdrs
        .iter()
        .find(|p| p.name.text == word)
        .map(|p| (i, p))
    }) {
      let source = &family.members[i].loaded.text[phdr.span.clone()];
      let from = me.unwrap_or(i);
      return Ok(Some(format!(
        "{}\n\nProgram header (segment)\n\n*Declared {}*",
        code_block(source),
        family.place(i, phdr.name.span.start, from)
      )));
    }
    if let Some((_, text)) = docs::PHDR_TYPES.iter().find(|(name, _)| *name == word) {
      return Ok(Some(format!("**{word}** · program header type\n\n{text}")));
    }
    Ok(None)
  }

  fn provide_code_actions(
    &mut self,
    request: CodeActionRequest,
  ) -> Result<Vec<CodeAction>, String> {
    let Some((version, text)) = document_text(request.document) else {
      return Ok(Vec::new());
    };
    if version != request.version {
      return Ok(Vec::new());
    }
    let Some((path, family)) = self.names(request.document) else {
      return Ok(Vec::new());
    };
    let Some(i) = family.index(&path) else {
      return Ok(Vec::new());
    };
    let (first, last) = (
      offset(&text, request.range.start),
      offset(&text, request.range.end),
    );
    let mut actions = Vec::new();
    for finding in family.problems(i) {
      let Some(replacement) = finding.replacement else {
        continue;
      };
      if finding.span.end < first || finding.span.start > last {
        continue;
      }
      actions.push(CodeAction {
        title: format!("Change to `{replacement}`"),
        kind: Some("quickfix".into()),
        preferred: true,
        edits: vec![TextEdit {
          range: range(&text, &finding.span),
          text: replacement,
        }],
        command: None,
        argument: None,
      });
    }
    Ok(actions)
  }
}

register!(LinkerScript);

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn converts_positions() {
    let text = "ab\ncdé\nf";
    assert_eq!(position(text, 4), Position { line: 1, column: 1 });
    assert_eq!(offset(text, Position { line: 1, column: 1 }), 4);
    assert_eq!(
      offset(
        text,
        Position {
          line: 1,
          column: 99
        }
      ),
      7,
      "clamped to the line"
    );
    assert_eq!(offset(text, Position { line: 9, column: 0 }), text.len());
    let line = line_range("  x = 1;  \n", 1);
    assert_eq!((line.start.column, line.end.column), (2, 8));
    assert_eq!(word_at("x = _etext;", 6), Some(("_etext".into(), 4..10)));
  }
}
