//! A script and the scripts it's linked with: those it INCLUDEs and those
//! that INCLUDE it, and theirs. Regions, sections, and symbols one declares
//! are known in all of them, so `> FLASH` in `sections.ld` finds the MEMORY
//! in `memory.ld`. The problems the extension finds itself are here too.

use std::collections::BTreeSet;
use std::rc::Rc;

use crate::parse::{self, Name, RefKind, Script, Span, SymbolDef, evaluate};
use crate::paths;

/// A script's text and what it says.
#[derive(Debug)]
pub struct Loaded {
  pub text: String,
  pub script: Script,
}

impl Loaded {
  pub fn new(text: String) -> Rc<Self> {
    let script = parse::parse(&text);
    Rc::new(Self { text, script })
  }
}

pub struct Member {
  pub path: String,
  pub loaded: Rc<Loaded>,
}

pub struct Family {
  pub members: Vec<Member>,
  /// `includes[i]`: the members member `i` includes.
  includes: Vec<Vec<usize>>,
  /// INCLUDEs that weren't found: the member and the name.
  pub unresolved: Vec<(usize, Name)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
  Error,
  Warning,
}

/// A problem the extension finds in a script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
  pub span: Span,
  pub level: Level,
  pub message: String,
  /// What the span may have meant.
  pub replacement: Option<String>,
}

/// Loads a script by path: an open document's text, or the file's.
pub type Load<'a> = dyn FnMut(&str) -> Option<Rc<Loaded>> + 'a;

impl Family {
  /// `start` and the scripts linked with it, among `others` (the folder's
  /// scripts) and the files their INCLUDEs name.
  pub fn of(start: &str, others: &[String], folder: Option<&str>, load: &mut Load) -> Self {
    let all = Self::all(Some(start), others, folder, load);
    let Some(first) = all.index(start) else {
      return all;
    };
    // The members connected to `start`, either way.
    let mut keep = BTreeSet::from([first]);
    let mut queue = vec![first];
    while let Some(i) = queue.pop() {
      let linked = all.includes[i]
        .iter()
        .copied()
        .chain((0..all.members.len()).filter(|&j| all.includes[j].contains(&i)));
      for j in linked.collect::<Vec<_>>() {
        if keep.insert(j) {
          queue.push(j);
        }
      }
    }
    all.only(&keep)
  }

  /// Every script: `start` (if given), `others`, and what they include.
  pub fn all(
    start: Option<&str>,
    others: &[String],
    folder: Option<&str>,
    load: &mut Load,
  ) -> Self {
    let mut family = Self {
      members: Vec::new(),
      includes: Vec::new(),
      unresolved: Vec::new(),
    };
    for path in start.into_iter().chain(others.iter().map(String::as_str)) {
      if family.index(path).is_none()
        && let Some(loaded) = load(path)
      {
        family.members.push(Member {
          path: path.to_owned(),
          loaded,
        });
      }
    }
    // Resolve includes, loading files not listed (in other folders, say).
    let mut i = 0;
    while i < family.members.len() {
      let mut found = Vec::new();
      let includes = family.members[i].loaded.script.includes.clone();
      for include in includes {
        match family.resolve(i, &include.text, folder, load) {
          Some(j) => found.push(j),
          None => family.unresolved.push((i, include)),
        }
      }
      family.includes.push(found);
      i += 1;
    }
    family
  }

  /// Where member `from`'s INCLUDE of `name` is: next to it, in the open
  /// folder, or in a SEARCH_DIR.
  fn resolve(
    &mut self,
    from: usize,
    name: &str,
    folder: Option<&str>,
    load: &mut Load,
  ) -> Option<usize> {
    let (dir, _) = paths::split(&self.members[from].path);
    let mut candidates = vec![paths::join(dir, name)];
    if let Some(folder) = folder {
      candidates.push(paths::join(folder, name));
    }
    for member in &self.members {
      for search in &member.loaded.script.search_dirs {
        let base = folder.unwrap_or(dir);
        let search = paths::join(base, search.text.trim_start_matches('='));
        candidates.push(paths::join(&search, name));
      }
    }
    for candidate in candidates {
      if let Some(i) = self.index(&candidate) {
        return Some(i);
      }
      if let Some(loaded) = load(&candidate) {
        self.members.push(Member {
          path: candidate,
          loaded,
        });
        return Some(self.members.len() - 1);
      }
    }
    None
  }

  fn only(self, keep: &BTreeSet<usize>) -> Self {
    let renumber: Vec<Option<usize>> = (0..self.members.len())
      .map(|i| keep.contains(&i).then(|| keep.range(..i).count()))
      .collect();
    let includes = self
      .includes
      .iter()
      .enumerate()
      .filter(|(i, _)| keep.contains(i))
      .map(|(_, found)| found.iter().filter_map(|&j| renumber[j]).collect())
      .collect();
    let unresolved = self
      .unresolved
      .into_iter()
      .filter_map(|(i, name)| Some((renumber[i]?, name)))
      .collect();
    let members = self
      .members
      .into_iter()
      .enumerate()
      .filter(|(i, _)| keep.contains(i))
      .map(|(_, member)| member)
      .collect();
    Self {
      members,
      includes,
      unresolved,
    }
  }

  pub fn index(&self, path: &str) -> Option<usize> {
    self
      .members
      .iter()
      .position(|member| paths::same(&member.path, path))
  }

  /// Member `root` and what it includes, and what those include: what the
  /// linker reads when given `root`.
  pub fn closure(&self, root: usize) -> Self {
    let mut keep = BTreeSet::from([root]);
    let mut queue = vec![root];
    while let Some(i) = queue.pop() {
      for &j in &self.includes[i] {
        if keep.insert(j) {
          queue.push(j);
        }
      }
    }
    let members = self
      .members
      .iter()
      .map(|member| Member {
        path: member.path.clone(),
        loaded: member.loaded.clone(),
      })
      .collect();
    Self {
      members,
      includes: self.includes.clone(),
      unresolved: self.unresolved.clone(),
    }
    .only(&keep)
  }

  /// The members no other member includes: the scripts given to the linker.
  pub fn roots(&self) -> Vec<usize> {
    (0..self.members.len())
      .filter(|&i| !self.includes.iter().any(|found| found.contains(&i)))
      .collect()
  }

  pub fn scripts(&self) -> impl Iterator<Item = (usize, &Script)> {
    self
      .members
      .iter()
      .enumerate()
      .map(|(i, member)| (i, &member.loaded.script))
  }

  /// The names of every region and alias.
  pub fn region_names(&self) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for (_, script) in self.scripts() {
      for name in script
        .regions
        .iter()
        .map(|region| &region.name.text)
        .chain(script.aliases.iter().map(|alias| &alias.name.text))
      {
        if !names.contains(name) {
          names.push(name.clone());
        }
      }
    }
    names
  }

  pub fn section_names(&self) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for (_, script) in self.scripts() {
      for section in &script.sections {
        if !section.discard && !names.contains(&section.name.text) {
          names.push(section.name.text.clone());
        }
      }
    }
    names
  }

  pub fn phdr_names(&self) -> Vec<String> {
    self
      .scripts()
      .flat_map(|(_, script)| script.phdrs.iter().map(|phdr| phdr.name.text.clone()))
      .collect()
  }

  /// Every definition of a symbol, with its member.
  pub fn symbols(&self) -> Vec<(usize, &SymbolDef)> {
    self
      .scripts()
      .flat_map(|(i, script)| script.symbols.iter().map(move |symbol| (i, symbol)))
      .collect()
  }

  /// A region's origin and length, if they can be worked out.
  pub fn region_value(&self, name: &str) -> Option<(u64, u64)> {
    self.region_value_at(name, 0)
  }

  fn region_value_at(&self, name: &str, depth: u32) -> Option<(u64, u64)> {
    if depth > 8 {
      return None;
    }
    let region = self.scripts().find_map(|(_, script)| script.region(name))?;
    let symbol = |name: &str| self.symbol_value_at(name, depth + 1);
    let regions = |name: &str| self.region_value_at(name, depth + 1);
    Some((
      evaluate(&region.origin, &symbol, &regions)?,
      evaluate(&region.length, &symbol, &regions)?,
    ))
  }

  /// A symbol's value, if a plain `=` outside SECTIONS gives one that can
  /// be worked out.
  pub fn symbol_value(&self, name: &str) -> Option<u64> {
    self.symbol_value_at(name, 0)
  }

  fn symbol_value_at(&self, name: &str, depth: u32) -> Option<u64> {
    if depth > 8 {
      return None;
    }
    let definitions: Vec<_> = self
      .symbols()
      .into_iter()
      .filter(|(_, symbol)| symbol.name.text == name)
      .collect();
    // Only a symbol defined once, by `=`, outside a section.
    let [(_, symbol)] = definitions.as_slice() else {
      return None;
    };
    if symbol.operator != "=" || symbol.section.is_some() {
      return None;
    }
    let symbols = |name: &str| self.symbol_value_at(name, depth + 1);
    let regions = |name: &str| self.region_value_at(name, depth + 1);
    evaluate(&symbol.value, &symbols, &regions)
  }

  /// Symbols the scripts use but don't define: they'd come from objects.
  pub fn undefined_symbols(&self) -> Vec<String> {
    let defined: BTreeSet<&str> = self
      .symbols()
      .into_iter()
      .map(|(_, symbol)| symbol.name.text.as_str())
      .collect();
    let mut names = BTreeSet::new();
    for (_, script) in self.scripts() {
      for reference in &script.references {
        if reference.kind == RefKind::Symbol
          && !reference.in_defined
          && !defined.contains(reference.name.text.as_str())
        {
          names.insert(reference.name.text.clone());
        }
      }
    }
    names.into_iter().take(1000).collect()
  }

  pub fn asserts(&self) -> Vec<String> {
    self
      .scripts()
      .flat_map(|(_, script)| script.asserts.iter().map(|assert| assert.message.clone()))
      .collect()
  }

  pub fn has_syntax_errors(&self) -> bool {
    self.scripts().any(|(_, script)| !script.errors.is_empty())
  }

  /// The problems in member `i`.
  pub fn problems(&self, i: usize) -> Vec<Finding> {
    let script = &self.members[i].loaded.script;
    let mut found: Vec<Finding> = script
      .errors
      .iter()
      .map(|problem| Finding {
        span: problem.span.clone(),
        level: Level::Error,
        message: problem.message.clone(),
        replacement: problem.unknown.clone(),
      })
      .collect();
    let complete = self.unresolved.is_empty();
    let regions = self.region_names();
    let sections = self.section_names();
    let phdrs = self.phdr_names();
    let has_phdrs = self.scripts().any(|(_, script)| script.has_phdrs);
    let inserts = self.scripts().any(|(_, script)| script.inserts);
    for reference in &script.references {
      let name = &reference.name.text;
      match reference.kind {
        RefKind::Region if complete && !regions.contains(name) => {
          let replacement = suggest_from(name, &regions);
          let message = if regions.is_empty() {
            format!("memory region `{name}` isn't declared: there's no MEMORY command")
          } else {
            format!(
              "memory region `{name}` isn't declared (MEMORY declares {})",
              regions.join(", ")
            )
          };
          found.push(Finding {
            span: reference.name.span.clone(),
            level: Level::Error,
            message,
            replacement,
          });
        }
        RefKind::Phdr if complete && !phdrs.contains(name) => {
          let message = if has_phdrs {
            format!("program header `{name}` isn't declared in PHDRS")
          } else {
            format!("`:{name}` names a program header, but there's no PHDRS command")
          };
          found.push(Finding {
            span: reference.name.span.clone(),
            level: Level::Error,
            message,
            replacement: suggest_from(name, &phdrs),
          });
        }
        RefKind::Section if complete && !inserts && !sections.contains(name) => {
          found.push(Finding {
            span: reference.name.span.clone(),
            level: Level::Error,
            message: format!("there's no output section `{name}`"),
            replacement: suggest_from(name, &sections),
          });
        }
        _ => {}
      }
    }
    // Regions declared twice.
    for region in &script.regions {
      let earlier = self.scripts().find_map(|(j, other)| {
        other
          .regions
          .iter()
          .find(|r| {
            r.name.text == region.name.text
              && (j != i || r.name.span.start < region.name.span.start)
          })
          .map(|r| (j, r.name.span.start))
      });
      if let Some((j, at)) = earlier {
        let place = self.place(j, at, i);
        found.push(Finding {
          span: region.name.span.clone(),
          level: Level::Error,
          message: format!(
            "memory region `{}` is already declared {place}",
            region.name.text
          ),
          replacement: None,
        });
      }
    }
    for (j, name) in &self.unresolved {
      if *j == i {
        found.push(Finding {
          span: name.span.clone(),
          level: Level::Warning,
          message: format!(
            "`{}` isn't found next to this script or in the open folder, so what it declares isn't known here",
            name.text
          ),
          replacement: None,
        });
      }
    }
    found.sort_by_key(|finding| (finding.span.start, finding.span.end));
    found
  }

  /// `on line N`, or `in file.ld, line N`, seen from member `from`.
  pub fn place(&self, member: usize, offset: usize, from: usize) -> String {
    let text = &self.members[member].loaded.text;
    let line = text[..offset.min(text.len())].matches('\n').count() + 1;
    if member == from {
      format!("on line {line}")
    } else {
      let (_, name) = paths::split(&self.members[member].path);
      format!("in {name}, line {line}")
    }
  }
}

fn suggest_from(name: &str, known: &[String]) -> Option<String> {
  let known: Vec<&str> = known.iter().map(String::as_str).collect();
  // Region and section names aren't upper case only: compare as typed.
  let limit = (name.len() / 3).clamp(1, 3);
  known
    .iter()
    .map(|candidate| (distance(name, candidate), *candidate))
    .filter(|(distance, _)| *distance <= limit)
    .min_by_key(|(distance, _)| *distance)
    .map(|(_, candidate)| candidate.to_owned())
}

fn distance(a: &str, b: &str) -> usize {
  parse::distance(&a.to_ascii_lowercase(), &b.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
  use std::collections::BTreeMap;

  use super::*;

  fn files(list: &[(&str, &str)]) -> BTreeMap<String, Rc<Loaded>> {
    list
      .iter()
      .map(|(path, text)| ((*path).to_owned(), Loaded::new((*text).to_owned())))
      .collect()
  }

  fn family(start: &str, files: &BTreeMap<String, Rc<Loaded>>) -> Family {
    let others: Vec<String> = files.keys().cloned().collect();
    Family::of(start, &others, Some("/fw"), &mut |path: &str| {
      files.get(path).cloned()
    })
  }

  fn messages(family: &Family, path: &str) -> Vec<String> {
    let i = family.index(path).expect("a member");
    family
      .problems(i)
      .into_iter()
      .map(|finding| finding.message)
      .collect()
  }

  const MEMORY: &str = "MEMORY {\n  FLASH (rx) : ORIGIN = FLASH_BASE, LENGTH = 64K\n  RAM (rwx) : ORIGIN = 0x20000000, LENGTH = 8K\n}\nFLASH_BASE = 0x08000000;\nREGION_ALIAS(\"TEXT\", FLASH);\n";

  #[test]
  fn included_scripts_share_their_regions() {
    let files = files(&[
      ("/fw/main.ld", "INCLUDE memory.ld\nINCLUDE sections.ld\n"),
      ("/fw/memory.ld", MEMORY),
      (
        "/fw/sections.ld",
        "SECTIONS {\n  .text : { *(.text) } > TEXT\n  .data : { *(.data) } > RAM AT> FLAHS\n}\n",
      ),
      (
        "/fw/other/unrelated.ld",
        "SECTIONS { .x : { *(.x) } > NOWHERE }",
      ),
    ]);
    let family = family("/fw/sections.ld", &files);
    assert_eq!(family.members.len(), 3, "not the unrelated script");
    let main = family.index("/fw/main.ld").expect("main");
    assert_eq!(family.roots(), [main]);
    assert_eq!(family.closure(main).members.len(), 3);
    assert_eq!(
      messages(&family, "/fw/sections.ld"),
      ["memory region `FLAHS` isn't declared (MEMORY declares FLASH, RAM, TEXT)"]
    );
    let i = family.index("/fw/sections.ld").expect("sections");
    assert_eq!(family.problems(i)[0].replacement.as_deref(), Some("FLASH"));
    assert_eq!(family.region_value("FLASH"), Some((0x0800_0000, 64 * 1024)));
    assert_eq!(family.region_value("TEXT"), Some((0x0800_0000, 64 * 1024)));
  }

  #[test]
  fn each_root_is_checked_with_what_it_includes() {
    let files = files(&[
      ("/fw/memory.ld", MEMORY),
      (
        "/fw/good.ld",
        "INCLUDE memory.ld
SECTIONS { .t : { *(.t) } > RAM }",
      ),
      (
        "/fw/broken.ld",
        "INCLUDE memory.ld
ENTRYY(x)",
      ),
    ]);
    let family = family("/fw/good.ld", &files);
    assert_eq!(family.roots().len(), 2, "both include memory.ld");
    assert!(family.has_syntax_errors());
    let good = family.index("/fw/good.ld").expect("good");
    let closure = family.closure(good);
    assert!(
      !closure.has_syntax_errors(),
      "broken.ld isn't linked with good.ld"
    );
    assert_eq!(closure.members.len(), 2);
    assert_eq!(
      closure.roots(),
      [closure.index("/fw/good.ld").expect("good")]
    );
  }

  #[test]
  fn unknown_includes_turn_off_region_checks() {
    let files = files(&[(
      "/fw/sections.ld",
      "INCLUDE gen/memory.ld\nSECTIONS { .t : { *(.t) } > FLASH }",
    )]);
    let family = family("/fw/sections.ld", &files);
    assert_eq!(
      messages(&family, "/fw/sections.ld"),
      [
        "`gen/memory.ld` isn't found next to this script or in the open folder, so what it declares isn't known here"
      ]
    );
  }

  #[test]
  fn finds_problems_with_names() {
    let files = files(&[(
      "/fw/a.ld",
      "MEMORY { RAM : ORIGIN = 0, LENGTH = 1K\n RAM : ORIGIN = 0, LENGTH = 1K }\nSECTIONS {\n  .text : { *(.text) } :text\n  x = SIZEOF(.txt);\n}\n",
    )]);
    let family = family("/fw/a.ld", &files);
    assert_eq!(
      messages(&family, "/fw/a.ld"),
      [
        "memory region `RAM` is already declared on line 1",
        "`:text` names a program header, but there's no PHDRS command",
        "there's no output section `.txt`",
      ]
    );
  }

  #[test]
  fn knows_undefined_symbols() {
    let files = files(&[(
      "/fw/a.ld",
      "_estack = ORIGIN(RAM) + LENGTH(RAM);\nSECTIONS { .t : { *(.t) } x = _estack + __heap + (DEFINED(opt) ? opt : 0); }",
    )]);
    let family = family("/fw/a.ld", &files);
    assert_eq!(family.undefined_symbols(), ["__heap", "opt"]);
  }
}
