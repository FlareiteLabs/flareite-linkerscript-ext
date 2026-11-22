//! A parser for GNU ld linker scripts, after ld's own grammar (ld/ldgram.y),
//! for what the extension needs: syntax errors, the regions, sections,
//! symbols, and program headers a script defines, the names it refers to,
//! and the expressions that give regions their origins and lengths.
//!
//! Scripts are read as ld reads them: a name is a letter, `_`, or `.`
//! followed by letters, digits, `_`, `.`, `$`, and `-` (so `a-b` is one
//! name), and file names and wildcard patterns run until a space or one of
//! `(){};,"=`. Positions are byte offsets.

use std::ops::Range;

pub type Span = Range<usize>;

/// Most syntax errors reported for one script.
const MAX_ERRORS: usize = 50;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
  pub span: Span,
  pub message: String,
  /// A name ld doesn't know, and what it may have meant.
  pub unknown: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Name {
  pub text: String,
  pub span: Span,
}

/// An expression, enough to evaluate regions' origins and lengths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
  Number(u64),
  Symbol(String),
  Dot,
  String(String),
  Call(String, Vec<Expr>),
  Unary(char, Box<Expr>),
  Binary(&'static str, Box<Expr>, Box<Expr>),
  Conditional(Box<Expr>, Box<Expr>, Box<Expr>),
  /// Couldn't be parsed.
  Invalid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Region {
  pub name: Name,
  pub attributes: String,
  pub origin: Expr,
  pub length: Expr,
  /// The whole definition.
  pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alias {
  pub name: Name,
  pub region: Name,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
  pub name: Name,
  /// From `/DISCARD/`.
  pub discard: bool,
  pub region: Option<Name>,
  pub load_region: Option<Name>,
  pub phdrs: Vec<Name>,
  /// ONLY_IF_RO or ONLY_IF_RW: ld keeps one of two such sections.
  pub constrained: bool,
  /// In an OVERLAY.
  pub overlay: bool,
  /// The whole definition, from its name to its last attribute.
  pub span: Span,
  /// The line it's on and the text after it: `.text : ALIGN(4K)`.
  pub header: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymbolKind {
  Assigned,
  Provided,
  Hidden,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolDef {
  pub name: Name,
  pub kind: SymbolKind,
  /// `=`, or `+=` and the like.
  pub operator: String,
  pub value: Expr,
  /// The text of the statement.
  pub source: String,
  /// The output section it's assigned in.
  pub section: Option<String>,
  pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Phdr {
  pub name: Name,
  pub kind: String,
  pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefKind {
  Region,
  Section,
  Phdr,
  Symbol,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
  pub name: Name,
  pub kind: RefKind,
  /// Inside `DEFINED(...)`, which asks whether it's defined.
  pub in_defined: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assert {
  pub message: String,
  pub span: Span,
}

/// What a script says.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Script {
  pub errors: Vec<Problem>,
  pub regions: Vec<Region>,
  pub aliases: Vec<Alias>,
  pub sections: Vec<Section>,
  pub symbols: Vec<SymbolDef>,
  pub phdrs: Vec<Phdr>,
  /// Whether it has a PHDRS command.
  pub has_phdrs: bool,
  /// Whether it INSERTs its sections into ld's default script, which has
  /// sections of its own.
  pub inserts: bool,
  pub references: Vec<Reference>,
  pub includes: Vec<Name>,
  pub search_dirs: Vec<Name>,
  pub entry: Option<Name>,
  pub asserts: Vec<Assert>,
  /// OUTPUT_FORMAT's names (default, big-endian, little-endian).
  pub formats: Vec<String>,
  pub arch: Option<String>,
}

/// Commands at the top of a script.
pub const TOP_COMMANDS: &[&str] = &[
  "ENTRY",
  "INCLUDE",
  "INPUT",
  "GROUP",
  "OUTPUT",
  "SEARCH_DIR",
  "STARTUP",
  "OUTPUT_FORMAT",
  "OUTPUT_ARCH",
  "TARGET",
  "EXTERN",
  "NOCROSSREFS",
  "NOCROSSREFS_TO",
  "REGION_ALIAS",
  "LD_FEATURE",
  "INSERT",
  "FORCE_COMMON_ALLOCATION",
  "INHIBIT_COMMON_ALLOCATION",
  "FORCE_GROUP_ALLOCATION",
  "PROVIDE",
  "PROVIDE_HIDDEN",
  "HIDDEN",
  "ASSERT",
  "SECTIONS",
  "MEMORY",
  "PHDRS",
  "VERSION",
];

/// Commands in SECTIONS, besides output sections and assignments.
pub const SECTIONS_COMMANDS: &[&str] = &[
  "ENTRY",
  "INCLUDE",
  "ASSERT",
  "PROVIDE",
  "PROVIDE_HIDDEN",
  "HIDDEN",
  "OVERLAY",
];

/// Commands in an output section, besides input sections and assignments.
pub const BODY_COMMANDS: &[&str] = &[
  "KEEP",
  "SORT",
  "SORT_BY_NAME",
  "SORT_BY_ALIGNMENT",
  "SORT_BY_INIT_PRIORITY",
  "SORT_NONE",
  "REVERSE",
  "EXCLUDE_FILE",
  "INPUT_SECTION_FLAGS",
  "BYTE",
  "SHORT",
  "LONG",
  "QUAD",
  "SQUAD",
  "FILL",
  "ASCIZ",
  "CREATE_OBJECT_SYMBOLS",
  "CONSTRUCTORS",
  "LINKER_VERSION",
  "INCLUDE",
  "ASSERT",
  "PROVIDE",
  "PROVIDE_HIDDEN",
  "HIDDEN",
];

/// Built-in functions of expressions.
pub const FUNCTIONS: &[&str] = &[
  "ABSOLUTE",
  "ADDR",
  "ALIGN",
  "ALIGNOF",
  "ASSERT",
  "BLOCK",
  "CONSTANT",
  "DATA_SEGMENT_ALIGN",
  "DATA_SEGMENT_END",
  "DATA_SEGMENT_RELRO_END",
  "DEFINED",
  "LENGTH",
  "LOADADDR",
  "LOG2CEIL",
  "MAX",
  "MIN",
  "NEXT",
  "ORIGIN",
  "SEGMENT_START",
  "SIZEOF",
];

/// Names expressions use that aren't symbols.
pub const CONSTANTS: &[&str] = &["SIZEOF_HEADERS", "MAXPAGESIZE", "COMMONPAGESIZE"];

/// Functions whose argument is an output section.
const SECTION_FUNCTIONS: &[&str] = &["ADDR", "ALIGNOF", "LOADADDR", "SIZEOF"];
/// Functions whose argument is a memory region.
const REGION_FUNCTIONS: &[&str] = &["ORIGIN", "LENGTH"];

const SECTION_TYPES: &[&str] = &[
  "NOLOAD", "DSECT", "COPY", "INFO", "OVERLAY", "READONLY", "TYPE",
];

const ASSIGN_OPS: &[&str] = &["<<=", ">>=", "+=", "-=", "*=", "/=", "&=", "|=", "^=", "="];

fn is_name_start(c: u8) -> bool {
  c.is_ascii_alphabetic() || c == b'_' || c == b'.'
}

pub fn is_name_char(c: u8) -> bool {
  c.is_ascii_alphanumeric() || matches!(c, b'_' | b'.' | b'$' | b'-')
}

fn is_file_char(c: u8) -> bool {
  !c.is_ascii_whitespace() && !matches!(c, b'(' | b')' | b'{' | b'}' | b';' | b',' | b'"' | b'=')
}

/// Where the parser is, for what an error says to expect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Block {
  Top,
  Sections,
  Body,
}

struct Parser<'a> {
  text: &'a str,
  bytes: &'a [u8],
  at: usize,
  script: Script,
  /// The output section being read.
  section: Option<String>,
  /// Inside DEFINED(...).
  in_defined: bool,
}

/// Parses a script.
pub fn parse(text: &str) -> Script {
  let mut parser = Parser {
    text,
    bytes: text.as_bytes(),
    at: 0,
    script: Script::default(),
    section: None,
    in_defined: false,
  };
  parser.commands();
  parser.script
}

/// Why a statement couldn't be read; recorded already.
struct Failed;

type Parsed<T> = Result<T, Failed>;

impl Parser<'_> {
  // ----- Characters -----

  fn peek(&self) -> Option<u8> {
    self.bytes.get(self.at).copied()
  }

  fn peek_at(&self, offset: usize) -> Option<u8> {
    self.bytes.get(self.at + offset).copied()
  }

  /// Skips spaces and comments (`/* */`, and `#` to the end of the line as
  /// lld allows). An unclosed comment is an error.
  fn skip(&mut self) {
    loop {
      while self.peek().is_some_and(|c| c.is_ascii_whitespace()) {
        self.at += 1;
      }
      if self.text[self.at..].starts_with("/*") {
        match self.text[self.at + 2..].find("*/") {
          Some(end) => self.at += 2 + end + 2,
          None => {
            let start = self.at;
            self.at = self.bytes.len();
            self.error(start..start + 2, "this comment isn't closed with */");
            return;
          }
        }
      } else if self.peek() == Some(b'#') {
        while self.peek().is_some_and(|c| c != b'\n') {
          self.at += 1;
        }
      } else {
        return;
      }
    }
  }

  fn at_end(&mut self) -> bool {
    self.skip();
    self.at >= self.bytes.len()
  }

  /// Takes `token` if it's next (after spaces).
  fn eat(&mut self, token: &str) -> bool {
    self.skip();
    if self.text[self.at..].starts_with(token) {
      self.at += token.len();
      true
    } else {
      false
    }
  }

  fn next_is(&mut self, token: &str) -> bool {
    self.skip();
    self.text[self.at..].starts_with(token)
  }

  /// The next character after spaces, and where it is.
  fn next_char(&mut self) -> Option<(usize, u8)> {
    self.skip();
    self.peek().map(|c| (self.at, c))
  }

  fn error(&mut self, span: Span, message: impl Into<String>) {
    if self.script.errors.len() < MAX_ERRORS {
      self.script.errors.push(Problem {
        span,
        message: message.into(),
        unknown: None,
      });
    }
  }

  /// An error at what's next: the token there, or the end.
  fn expected(&mut self, what: &str) -> Failed {
    self.skip();
    let span = self.token_span();
    let found = if span.is_empty() {
      "the end of the file".to_owned()
    } else {
      format!("`{}`", &self.text[span.clone()])
    };
    let span = if span.is_empty() {
      // The end: mark the last character.
      self.text.len().saturating_sub(1)..self.text.len()
    } else {
      span
    };
    self.error(span, format!("expected {what}, found {found}"));
    Failed
  }

  /// The span of the token at the cursor, for errors.
  fn token_span(&self) -> Span {
    let start = self.at;
    let Some(c) = self.peek() else {
      return start..start;
    };
    let mut end = start + 1;
    if is_name_char(c) || c == b'"' {
      while self.bytes.get(end).is_some_and(|&c| is_name_char(c)) {
        end += 1;
      }
    }
    while !self.text.is_char_boundary(end) {
      end += 1;
    }
    start..end.min(self.text.len())
  }

  fn expect(&mut self, token: &str) -> Parsed<()> {
    if self.eat(token) {
      Ok(())
    } else {
      Err(self.expected(&format!("`{token}`")))
    }
  }

  // ----- Tokens -----

  /// A name (`.text`, `_start`), if one is next.
  fn name(&mut self) -> Option<Name> {
    self.skip();
    let start = self.at;
    if !self.peek().is_some_and(is_name_start) {
      return None;
    }
    let mut end = start + 1;
    while self.bytes.get(end).is_some_and(|&c| is_name_char(c)) {
      end += 1;
    }
    // A name ends before an assignment operator: `a-=1` is `a -= 1`.
    for op in ["-=", "+="] {
      if self.text[start..end].ends_with(&op[..1]) && self.bytes.get(end) == Some(&b'=') {
        end -= 1;
      }
    }
    self.at = end;
    Some(Name {
      text: self.text[start..end].to_owned(),
      span: start..end,
    })
  }

  fn peek_name(&mut self) -> Option<String> {
    let at = self.at;
    let name = self.name().map(|name| name.text);
    self.at = at;
    name
  }

  /// A quoted string, without its quotes.
  fn string(&mut self) -> Option<Name> {
    self.skip();
    if self.peek() != Some(b'"') {
      return None;
    }
    let start = self.at;
    match self.text[start + 1..].find(['"', '\n']) {
      Some(end) if self.bytes[start + 1 + end] == b'"' => {
        self.at = start + 1 + end + 1;
        Some(Name {
          text: self.text[start + 1..start + 1 + end].to_owned(),
          span: start..self.at,
        })
      }
      _ => {
        let end = self.text[start..]
          .find('\n')
          .map_or(self.text.len(), |i| start + i);
        self.at = end;
        self.error(start..end, "this string isn't closed with \"");
        Some(Name {
          text: self.text[start + 1..end].to_owned(),
          span: start..end,
        })
      }
    }
  }

  /// A file name or wildcard pattern (`*crt0.o`, `.text.*`), or a string.
  fn file(&mut self) -> Option<Name> {
    if let Some(string) = self.string() {
      return Some(string);
    }
    let start = self.at;
    let mut end = start;
    while self.bytes.get(end).is_some_and(|&c| is_file_char(c)) {
      end += 1;
    }
    while !self.text.is_char_boundary(end) {
      end += 1;
    }
    (end > start).then(|| {
      self.at = end;
      Name {
        text: self.text[start..end].to_owned(),
        span: start..end,
      }
    })
  }

  /// A name or a string (a quoted symbol or section name).
  fn name_or_string(&mut self) -> Option<Name> {
    self.string().or_else(|| self.name())
  }

  /// An assignment operator, if one is next.
  fn assign_op(&mut self) -> Option<&'static str> {
    self.skip();
    let rest = &self.text[self.at..];
    let op = ASSIGN_OPS.iter().find(|op| rest.starts_with(**op))?;
    // `==` is a comparison.
    if *op == "=" && rest.starts_with("==") {
      return None;
    }
    self.at += op.len();
    Some(op)
  }

  // ----- Recovery -----

  /// Skips the rest of a broken statement: to a `;` (taken), a `}` (left),
  /// or a line starting with a command of `block`.
  fn recover(&mut self, block: Block) {
    let mut depth = 0usize;
    let start_line = self.line_of(self.at);
    while let Some((at, c)) = self.next_char() {
      match c {
        b'(' => depth += 1,
        b')' => depth = depth.saturating_sub(1),
        b';' if depth == 0 => {
          self.at += 1;
          return;
        }
        b'}' => return,
        b'{' => {
          // A block: skip it whole.
          self.at += 1;
          self.skip_block();
          continue;
        }
        _ if depth == 0 && self.line_of(at) != start_line && self.starts_statement(block) => {
          return;
        }
        _ => {}
      }
      if c == b'"' {
        let _ = self.string();
      } else if is_name_start(c) {
        let _ = self.name();
      } else {
        self.at += 1;
        while !self.text.is_char_boundary(self.at) {
          self.at += 1;
        }
      }
    }
  }

  /// Skips to after the `}` closing a block whose `{` was just taken.
  fn skip_block(&mut self) {
    let mut depth = 1usize;
    while let Some((_, c)) = self.next_char() {
      self.at += 1;
      match c {
        b'{' => depth += 1,
        b'}' => {
          depth -= 1;
          if depth == 0 {
            return;
          }
        }
        b'"' => {
          self.at -= 1;
          let _ = self.string();
        }
        _ => {}
      }
      while !self.text.is_char_boundary(self.at) {
        self.at += 1;
      }
    }
  }

  fn line_of(&self, offset: usize) -> usize {
    self.text[..offset].matches('\n').count()
  }

  /// Whether what's next looks like the start of a statement in `block`:
  /// one of its commands, or a name followed by `=` or `:`.
  fn starts_statement(&mut self, block: Block) -> bool {
    let at = self.at;
    let starts = match self.name() {
      Some(name) => {
        let commands = match block {
          Block::Top => TOP_COMMANDS,
          Block::Sections => SECTIONS_COMMANDS,
          Block::Body => BODY_COMMANDS,
        };
        commands.contains(&name.text.as_str())
          || self.assign_op().is_some()
          || (block == Block::Sections && self.next_is(":"))
      }
      None => false,
    };
    self.at = at;
    starts
  }

  // ----- Statements -----

  /// The commands at the top of the script.
  fn commands(&mut self) {
    while !self.at_end() {
      let before = self.at;
      if self.eat(";") {
        continue;
      }
      if self.next_is("}") {
        let span = self.at..self.at + 1;
        self.error(span, "this `}` closes nothing");
        self.at += 1;
        continue;
      }
      if self.top_command().is_err() {
        self.recover(Block::Top);
      }
      if self.at == before {
        // Never stall.
        self.at += 1;
      }
    }
  }

  fn top_command(&mut self) -> Parsed<()> {
    let at = self.at;
    let Some(word) = self.name_or_string() else {
      return Err(self.expected("a command"));
    };
    let is_string = self.text[word.span.clone()].starts_with('"');
    match word.text.as_str() {
      _ if is_string => {
        self.at = at;
        self.assignment()
      }
      "SECTIONS" => self.sections(),
      "MEMORY" => self.memory(),
      "PHDRS" => self.phdrs(),
      "VERSION" => {
        self.expect("{")?;
        self.skip_block();
        Ok(())
      }
      "ENTRY" => self.entry(),
      "INCLUDE" => self.include(),
      "INPUT" | "GROUP" => self.files(true),
      "OUTPUT" | "STARTUP" | "TARGET" => self.one_file(),
      "SEARCH_DIR" => {
        self.expect("(")?;
        let dir = self.file().ok_or_else(|| self.expected("a folder"))?;
        self.script.search_dirs.push(dir);
        self.expect(")")
      }
      "OUTPUT_FORMAT" => {
        self.expect("(")?;
        loop {
          let format = self
            .file()
            .ok_or_else(|| self.expected("an output format"))?;
          self.script.formats.push(format.text);
          if !self.eat(",") {
            break;
          }
        }
        self.expect(")")
      }
      "OUTPUT_ARCH" => {
        self.expect("(")?;
        let arch = self
          .file()
          .ok_or_else(|| self.expected("an architecture"))?;
        self.script.arch = Some(arch.text);
        self.expect(")")
      }
      "EXTERN" => {
        self.expect("(")?;
        while !self.eat(")") {
          if self.eat(",") {
            continue;
          }
          self
            .name()
            .ok_or_else(|| self.expected("a symbol or `)`"))?;
        }
        Ok(())
      }
      "NOCROSSREFS" | "NOCROSSREFS_TO" => {
        self.expect("(")?;
        while !self.eat(")") {
          if self.eat(",") {
            continue;
          }
          let name = self
            .name()
            .ok_or_else(|| self.expected("a section or `)`"))?;
          self.reference(name, RefKind::Section);
        }
        Ok(())
      }
      "REGION_ALIAS" => {
        self.expect("(")?;
        let name = self
          .name_or_string()
          .ok_or_else(|| self.expected("the alias"))?;
        self.expect(",")?;
        let region = self.name().ok_or_else(|| self.expected("a region"))?;
        self.reference(region.clone(), RefKind::Region);
        self.script.aliases.push(Alias { name, region });
        self.expect(")")
      }
      "LD_FEATURE" => {
        self.expect("(")?;
        self.string().ok_or_else(|| self.expected("a string"))?;
        self.expect(")")
      }
      "INSERT" => {
        self.script.inserts = true;
        match self.name().as_ref().map(|name| name.text.as_str()) {
          Some("AFTER" | "BEFORE") => {}
          _ => return Err(self.expected("AFTER or BEFORE")),
        }
        let section = self.name().ok_or_else(|| self.expected("a section"))?;
        self.reference(section, RefKind::Section);
        Ok(())
      }
      "FORCE_COMMON_ALLOCATION" | "INHIBIT_COMMON_ALLOCATION" | "FORCE_GROUP_ALLOCATION" => Ok(()),
      "PROVIDE" | "PROVIDE_HIDDEN" | "HIDDEN" => self.provide(&word.text, at),
      "ASSERT" => self.assert(at),
      _ => {
        self.at = at;
        if self.is_assignment() {
          return self.assignment();
        }
        self.at = word.span.end;
        Err(self.unknown(word, TOP_COMMANDS))
      }
    }
  }

  /// A name ld doesn't know where a command should be.
  fn unknown(&mut self, word: Name, commands: &[&str]) -> Failed {
    let suggestion = suggest(&word.text, commands);
    let mut message = format!("`{}` isn't a command here", word.text);
    if let Some(suggestion) = &suggestion {
      message.push_str(&format!("; did you mean `{suggestion}`?"));
    } else if self.next_is(":") || self.next_is("{") {
      message.push_str(" (output sections go in SECTIONS)");
    } else {
      message.push_str(" (an assignment needs `=`)");
    }
    if self.script.errors.len() < MAX_ERRORS {
      self.script.errors.push(Problem {
        span: word.span,
        message,
        unknown: suggestion,
      });
    }
    Failed
  }

  /// Whether a symbol assignment is next (leaves the position).
  fn is_assignment(&mut self) -> bool {
    let at = self.at;
    let target = self.eat_dot() || self.name_or_string().is_some();
    let assignment = target && self.assign_op().is_some();
    self.at = at;
    assignment
  }

  /// `.` alone: the location counter.
  fn eat_dot(&mut self) -> bool {
    self.skip();
    if self.peek() == Some(b'.') && !self.peek_at(1).is_some_and(is_name_char) {
      self.at += 1;
      true
    } else {
      false
    }
  }

  /// `symbol = expression;` (or `. = ...`).
  fn assignment(&mut self) -> Parsed<()> {
    self.skip();
    let start = self.at;
    let name = if self.eat_dot() {
      None
    } else {
      Some(
        self
          .name_or_string()
          .ok_or_else(|| self.expected("a symbol"))?,
      )
    };
    let operator = self
      .assign_op()
      .ok_or_else(|| self.expected("`=`"))?
      .to_owned();
    if let Some(name) = &name
      && operator != "="
    {
      self.reference(name.clone(), RefKind::Symbol);
    }
    let value = self.expression()?;
    if !self.eat(";") && !self.eat(",") {
      return Err(self.expected("`;` after the assignment"));
    }
    if let Some(name) = name {
      self.script.symbols.push(SymbolDef {
        name,
        kind: SymbolKind::Assigned,
        operator,
        value,
        source: self.text[start..self.at].to_owned(),
        section: self.section.clone(),
        span: start..self.at,
      });
    }
    Ok(())
  }

  /// PROVIDE(symbol = expression), PROVIDE_HIDDEN, or HIDDEN; the command's
  /// name is read.
  fn provide(&mut self, command: &str, start: usize) -> Parsed<()> {
    self.expect("(")?;
    let name = self
      .name_or_string()
      .ok_or_else(|| self.expected("a symbol"))?;
    self.expect("=")?;
    let value = self.expression()?;
    self.expect(")")?;
    self.script.symbols.push(SymbolDef {
      name,
      kind: if command == "HIDDEN" {
        SymbolKind::Hidden
      } else {
        SymbolKind::Provided
      },
      operator: "=".into(),
      value,
      source: self.text[start..self.at].to_owned(),
      section: self.section.clone(),
      span: start..self.at,
    });
    Ok(())
  }

  /// ASSERT(expression, "message"); the command's name is read.
  fn assert(&mut self, start: usize) -> Parsed<()> {
    self.expect("(")?;
    self.expression()?;
    self.expect(",")?;
    let message = self
      .name_or_string()
      .ok_or_else(|| self.expected("a message"))?;
    self.expect(")")?;
    self.script.asserts.push(Assert {
      message: message.text,
      span: start..self.at,
    });
    Ok(())
  }

  fn entry(&mut self) -> Parsed<()> {
    self.expect("(")?;
    let name = self.name().ok_or_else(|| self.expected("a symbol"))?;
    self.script.entry = Some(name);
    self.expect(")")
  }

  fn include(&mut self) -> Parsed<()> {
    let file = self.file().ok_or_else(|| self.expected("a file"))?;
    self.script.includes.push(file);
    Ok(())
  }

  /// INPUT(...) or GROUP(...): files, and AS_NEEDED(...) of them.
  fn files(&mut self, nested: bool) -> Parsed<()> {
    self.expect("(")?;
    while !self.eat(")") {
      if self.eat(",") {
        continue;
      }
      if nested && self.peek_name().as_deref() == Some("AS_NEEDED") {
        self.name();
        self.files(false)?;
        continue;
      }
      self.file().ok_or_else(|| self.expected("a file or `)`"))?;
    }
    Ok(())
  }

  fn one_file(&mut self) -> Parsed<()> {
    self.expect("(")?;
    self.file().ok_or_else(|| self.expected("a file"))?;
    self.expect(")")
  }

  // ----- MEMORY -----

  fn memory(&mut self) -> Parsed<()> {
    self.expect("{")?;
    loop {
      if self.eat("}") {
        return Ok(());
      }
      if self.at_end() {
        return Err(self.expected("`}` closing MEMORY"));
      }
      if self.eat(";") || self.eat(",") {
        continue;
      }
      if self.peek_name().as_deref() == Some("INCLUDE") {
        self.name();
        if self.include().is_err() {
          self.recover(Block::Top);
        }
        continue;
      }
      if self.region().is_err() {
        self.recover_in_memory();
      }
    }
  }

  /// Skips to the next line, or the `}` closing MEMORY.
  fn recover_in_memory(&mut self) {
    let line = self.line_of(self.at);
    while let Some((at, c)) = self.next_char() {
      if c == b'}' || self.line_of(at) != line {
        return;
      }
      self.at += 1;
      while !self.text.is_char_boundary(self.at) {
        self.at += 1;
      }
    }
  }

  /// `NAME (attributes) : ORIGIN = expression, LENGTH = expression`
  fn region(&mut self) -> Parsed<()> {
    self.skip();
    let start = self.at;
    let name = self
      .name()
      .ok_or_else(|| self.expected("a region's name"))?;
    let mut attributes = String::new();
    if self.eat("(") {
      self.skip();
      let from = self.at;
      while self.peek().is_some_and(|c| c != b')' && c != b'\n') {
        self.at += 1;
      }
      attributes = self.text[from..self.at].trim().to_owned();
      if let Some(bad) = attributes.chars().find(|c| !"rwxailRWXAIL! ".contains(*c)) {
        let span = from..self.at;
        self.error(
          span,
          format!("`{bad}` isn't a region attribute (r, w, x, a, i, l, and ! for not)"),
        );
      }
      self.expect(")")?;
    }
    self.expect(":")?;
    let origin_word = self.name();
    if !origin_word
      .as_ref()
      .is_some_and(|word| matches!(word.text.as_str(), "ORIGIN" | "org" | "o"))
    {
      if let Some(word) = &origin_word {
        self.at = word.span.start;
      }
      return Err(self.expected("ORIGIN (or org, or o)"));
    }
    self.expect("=")?;
    let origin = self.expression()?;
    self.eat(",");
    let length_word = self.name();
    if !length_word
      .as_ref()
      .is_some_and(|word| matches!(word.text.as_str(), "LENGTH" | "len" | "l"))
    {
      if let Some(word) = &length_word {
        self.at = word.span.start;
      }
      return Err(self.expected("LENGTH (or len, or l)"));
    }
    self.expect("=")?;
    let length = self.expression()?;
    self.script.regions.push(Region {
      name,
      attributes,
      origin,
      length,
      span: start..self.at,
    });
    Ok(())
  }

  // ----- PHDRS -----

  fn phdrs(&mut self) -> Parsed<()> {
    self.script.has_phdrs = true;
    self.expect("{")?;
    loop {
      if self.eat("}") {
        return Ok(());
      }
      if self.at_end() {
        return Err(self.expected("`}` closing PHDRS"));
      }
      if self.eat(";") {
        continue;
      }
      if self.phdr().is_err() {
        self.recover(Block::Top);
      }
    }
  }

  /// `name TYPE [FILEHDR] [PHDRS] [AT(address)] [FLAGS(flags)];`
  fn phdr(&mut self) -> Parsed<()> {
    self.skip();
    let start = self.at;
    let name = self
      .name()
      .ok_or_else(|| self.expected("a program header's name"))?;
    let kind = match self.name() {
      Some(kind) => kind.text,
      None => {
        self.expression()?;
        "number".into()
      }
    };
    loop {
      match self.peek_name().as_deref() {
        Some("FILEHDR" | "PHDRS") => {
          self.name();
        }
        Some("AT" | "FLAGS") => {
          self.name();
          self.expect("(")?;
          self.expression()?;
          self.expect(")")?;
        }
        _ => break,
      }
    }
    self.expect(";")?;
    self.script.phdrs.push(Phdr {
      name,
      kind,
      span: start..self.at,
    });
    Ok(())
  }

  // ----- SECTIONS -----

  fn sections(&mut self) -> Parsed<()> {
    self.expect("{")?;
    loop {
      if self.eat("}") {
        return Ok(());
      }
      if self.at_end() {
        return Err(self.expected("`}` closing SECTIONS"));
      }
      let before = self.at;
      if self.sections_item().is_err() {
        self.recover(Block::Sections);
      }
      if self.at == before {
        self.at += 1;
      }
    }
  }

  fn sections_item(&mut self) -> Parsed<()> {
    if self.eat(";") {
      return Ok(());
    }
    let at = self.at;
    if self.is_assignment() {
      return self.assignment();
    }
    if self.next_is("/DISCARD/") {
      let span = self.at..self.at + "/DISCARD/".len();
      self.at = span.end;
      let name = Name {
        text: "/DISCARD/".into(),
        span,
      };
      return self.output_section(name, true, false);
    }
    let Some(word) = self.name_or_string() else {
      return Err(self.expected("an output section, an assignment, or `}`"));
    };
    match word.text.as_str() {
      "ENTRY" => self.entry(),
      "INCLUDE" => self.include(),
      "ASSERT" if self.next_is("(") => self.assert(at),
      "PROVIDE" | "PROVIDE_HIDDEN" | "HIDDEN" if self.next_is("(") => self.provide(&word.text, at),
      "OVERLAY" => self.overlay(),
      _ => {
        // An output section has its address, type, or `:` next.
        let next = self.next_char().map(|(_, c)| c);
        if matches!(next, Some(b'{' | b';') | None) || self.next_is("}") {
          return Err(self.unknown(word, SECTIONS_COMMANDS));
        }
        if word
          .text
          .chars()
          .all(|c| c.is_ascii_uppercase() || c == '_')
          && self.next_is("(")
          && suggest(&word.text, SECTIONS_COMMANDS).is_some()
        {
          return Err(self.unknown(word, SECTIONS_COMMANDS));
        }
        self.output_section(word, false, false)
      }
    }
  }

  /// An output section, its name read: `[address] [(type)] : [AT(lma)]
  /// [ALIGN(a)] [SUBALIGN(a)] [constraint] { ... } [>region] [AT>region]
  /// [:phdr...] [=fill] [,]`.
  fn output_section(&mut self, name: Name, discard: bool, overlay: bool) -> Parsed<()> {
    let start = name.span.start;
    let mut constrained = false;
    if !overlay {
      // The type alone, or an address and then perhaps the type.
      if !self.section_type()? && !self.next_is(":") {
        self.expression()?;
        self.section_type()?;
      }
      self.expect(":")?;
      loop {
        match self.peek_name().as_deref() {
          Some("AT")
            if !self.text[self.at..]
              .trim_start_matches("AT")
              .trim_start()
              .starts_with('>') =>
          {
            self.name();
            self.expect("(")?;
            self.expression()?;
            self.expect(")")?;
          }
          Some("ALIGN" | "SUBALIGN") => {
            self.name();
            self.expect("(")?;
            self.expression()?;
            self.expect(")")?;
          }
          Some("ALIGN_WITH_INPUT" | "SPECIAL") => {
            self.name();
          }
          Some("ONLY_IF_RO" | "ONLY_IF_RW") => {
            self.name();
            constrained = true;
          }
          _ => break,
        }
      }
    }
    let line_end = self.text[start..]
      .find('\n')
      .map_or(self.text.len(), |i| start + i);
    let header = self.text[start..line_end.min(self.at.max(start))]
      .trim()
      .to_owned();
    if !self.eat("{") {
      return Err(self.expected("`{`"));
    }
    let previous = self.section.replace(name.text.clone());
    let body = self.body();
    self.section = previous;
    body?;
    let mut section = Section {
      name,
      discard,
      region: None,
      load_region: None,
      phdrs: Vec::new(),
      constrained,
      overlay,
      span: start..self.at,
      header,
    };
    if !overlay && self.eat(">") {
      let region = self
        .name()
        .ok_or_else(|| self.expected("a memory region"))?;
      self.reference(region.clone(), RefKind::Region);
      section.region = Some(region);
    }
    if !overlay && self.peek_name().as_deref() == Some("AT") {
      let at = self.at;
      self.name();
      if self.eat(">") {
        let region = self
          .name()
          .ok_or_else(|| self.expected("a memory region"))?;
        self.reference(region.clone(), RefKind::Region);
        section.load_region = Some(region);
      } else {
        self.at = at;
      }
    }
    self.phdr_list(&mut section.phdrs)?;
    if self.next_is("=") && !self.next_is("==") {
      self.eat("=");
      self.expression()?;
    }
    self.eat(",");
    section.span.end = self.at;
    self.script.sections.push(section);
    Ok(())
  }

  /// `:phdr` after a section: each a program header.
  fn phdr_list(&mut self, phdrs: &mut Vec<Name>) -> Parsed<()> {
    while self.next_is(":") {
      self.eat(":");
      let phdr = self
        .name()
        .ok_or_else(|| self.expected("a program header"))?;
      self.reference(phdr.clone(), RefKind::Phdr);
      phdrs.push(phdr);
    }
    Ok(())
  }

  /// `(NOLOAD)` and the other section types, if one is next.
  fn section_type(&mut self) -> Parsed<bool> {
    let at = self.at;
    if !self.eat("(") {
      return Ok(false);
    }
    match self.peek_name() {
      Some(word) if SECTION_TYPES.contains(&word.as_str()) => {}
      _ => {
        self.at = at;
        return Ok(false);
      }
    }
    let mut depth = 1;
    while depth > 0 {
      match self.next_char() {
        Some((_, b'(')) => depth += 1,
        Some((_, b')')) => depth -= 1,
        Some(_) => {}
        None => return Err(self.expected("`)`")),
      }
      self.at += 1;
    }
    Ok(true)
  }

  /// The statements of an output section, its `{` taken; takes its `}`.
  fn body(&mut self) -> Parsed<()> {
    loop {
      if self.eat("}") {
        return Ok(());
      }
      if self.at_end() {
        return Err(self.expected("`}` closing the output section"));
      }
      let before = self.at;
      if self.body_item().is_err() {
        self.recover(Block::Body);
        if self.at_end() {
          return Err(Failed);
        }
      }
      if self.at == before {
        self.at += 1;
      }
    }
  }

  fn body_item(&mut self) -> Parsed<()> {
    if self.eat(";") || self.eat(",") {
      return Ok(());
    }
    let at = self.at;
    if self.is_assignment() {
      return self.assignment();
    }
    if let Some(word) = self.peek_name() {
      let call = {
        self.name();
        let call = self.next_is("(");
        self.at = at;
        call
      };
      match word.as_str() {
        "KEEP" if call => {
          self.name();
          self.expect("(")?;
          while !self.eat(")") {
            if self.at_end() {
              return Err(self.expected("`)`"));
            }
            self.input_section()?;
          }
          return Ok(());
        }
        "BYTE" | "SHORT" | "LONG" | "QUAD" | "SQUAD" | "FILL" if call => {
          self.name();
          self.expect("(")?;
          self.expression()?;
          return self.expect(")");
        }
        "ASCIZ" => {
          self.name();
          self.string().ok_or_else(|| self.expected("a string"))?;
          return Ok(());
        }
        "CREATE_OBJECT_SYMBOLS" | "CONSTRUCTORS" | "LINKER_VERSION" => {
          self.name();
          return Ok(());
        }
        "INCLUDE" => {
          self.name();
          return self.include();
        }
        "ASSERT" if call => {
          self.name();
          return self.assert(at);
        }
        "PROVIDE" | "PROVIDE_HIDDEN" | "HIDDEN" if call => {
          self.name();
          return self.provide(&word, at);
        }
        _ => {}
      }
      // An all-caps name ld doesn't know, called like a command.
      if call
        && word.chars().all(|c| c.is_ascii_uppercase() || c == '_')
        && !is_sort(&word)
        && word != "EXCLUDE_FILE"
        && word != "INPUT_SECTION_FLAGS"
        && let Some(suggestion) = suggest(&word, BODY_COMMANDS)
      {
        let name = self.name().expect("peeked");
        let message = format!("`{word}` isn't a command here; did you mean `{suggestion}`?");
        if self.script.errors.len() < MAX_ERRORS {
          self.script.errors.push(Problem {
            span: name.span,
            message,
            unknown: Some(suggestion),
          });
        }
        return Err(Failed);
      }
    }
    self.input_section()
  }

  /// `[INPUT_SECTION_FLAGS(...)] file-pattern [( section-patterns )]`
  fn input_section(&mut self) -> Parsed<()> {
    if self.peek_name().as_deref() == Some("INPUT_SECTION_FLAGS") {
      self.name();
      self.expect("(")?;
      while !self.eat(")") {
        if self.at_end() {
          return Err(self.expected("`)`"));
        }
        self.at += 1;
      }
    }
    self.patterns_item(true)?;
    if self.eat("(") {
      while !self.eat(")") {
        if self.at_end() {
          return Err(self.expected("`)`"));
        }
        if self.eat(",") {
          continue;
        }
        self.patterns_item(false)?;
      }
    }
    Ok(())
  }

  /// A pattern, or SORT(...) or EXCLUDE_FILE(...) of patterns.
  fn patterns_item(&mut self, file: bool) -> Parsed<()> {
    self.skip();
    let at = self.at;
    if let Some(word) = self.peek_name()
      && (is_sort(&word) || word == "EXCLUDE_FILE")
    {
      self.name();
      if self.eat("(") {
        while !self.eat(")") {
          if self.at_end() {
            return Err(self.expected("`)`"));
          }
          if self.eat(",") {
            continue;
          }
          self.patterns_item(file)?;
        }
        // EXCLUDE_FILE(...) is followed by the patterns it excludes from.
        return Ok(());
      }
      self.at = at;
    }
    match self.file() {
      Some(_) => Ok(()),
      None => Err(self.expected(if file {
        "an input section (such as `*(.text)`), an assignment, or `}`"
      } else {
        "a section name or `)`"
      })),
    }
  }

  /// `OVERLAY [start] : [NOCROSSREFS] [AT(lma)] { name { ... } ... }
  /// [>region] [:phdr...] [=fill] [,]`; OVERLAY is read.
  fn overlay(&mut self) -> Parsed<()> {
    if !self.next_is(":") {
      self.expression()?;
    }
    self.expect(":")?;
    if self.peek_name().as_deref() == Some("NOCROSSREFS") {
      self.name();
    }
    if self.peek_name().as_deref() == Some("AT") {
      self.name();
      self.expect("(")?;
      self.expression()?;
      self.expect(")")?;
    }
    self.expect("{")?;
    loop {
      if self.eat("}") {
        break;
      }
      if self.at_end() {
        return Err(self.expected("`}` closing the OVERLAY"));
      }
      let name = self
        .name_or_string()
        .ok_or_else(|| self.expected("a section in the overlay"))?;
      self.output_section(name, false, true)?;
    }
    if self.eat(">") {
      let region = self
        .name()
        .ok_or_else(|| self.expected("a memory region"))?;
      self.reference(region, RefKind::Region);
    }
    let mut phdrs = Vec::new();
    self.phdr_list(&mut phdrs)?;
    if self.next_is("=") {
      self.eat("=");
      self.expression()?;
    }
    self.eat(",");
    Ok(())
  }

  // ----- Expressions -----

  fn reference(&mut self, name: Name, kind: RefKind) {
    self.script.references.push(Reference {
      name,
      kind,
      in_defined: self.in_defined,
    });
  }

  fn expression(&mut self) -> Parsed<Expr> {
    self.conditional()
  }

  fn conditional(&mut self) -> Parsed<Expr> {
    let condition = self.binary(0)?;
    if !self.eat("?") {
      return Ok(condition);
    }
    let consequence = self.expression()?;
    self.expect(":")?;
    let alternative = self.expression()?;
    Ok(Expr::Conditional(
      Box::new(condition),
      Box::new(consequence),
      Box::new(alternative),
    ))
  }

  /// The binary operator next, if it binds at least as tightly as `min`.
  fn binary_op(&mut self, min: u8) -> Option<(&'static str, u8)> {
    const OPS: &[(&str, u8)] = &[
      ("||", 1),
      ("&&", 2),
      ("==", 6),
      ("!=", 6),
      ("<=", 7),
      (">=", 7),
      ("<<", 9),
      (">>", 9),
      ("|", 3),
      ("^", 4),
      ("&", 5),
      ("<", 7),
      (">", 7),
      ("+", 10),
      ("-", 10),
      ("*", 11),
      ("/", 11),
      ("%", 11),
    ];
    self.skip();
    let rest = &self.text[self.at..];
    let (op, precedence) = OPS.iter().find(|(op, _)| rest.starts_with(op))?;
    // Not an assignment operator (`<<=`, `+=`) or `/DISCARD/`.
    let after = rest.as_bytes().get(op.len());
    if *precedence < min
      || (after == Some(&b'=') && !matches!(*op, "<=" | ">=" | "==" | "!="))
      || rest.starts_with("/DISCARD/")
    {
      return None;
    }
    Some((op, *precedence))
  }

  fn binary(&mut self, min: u8) -> Parsed<Expr> {
    let mut left = self.unary()?;
    while let Some((op, precedence)) = self.binary_op(min) {
      self.at += op.len();
      let right = self.binary(precedence + 1)?;
      left = Expr::Binary(op, Box::new(left), Box::new(right));
    }
    Ok(left)
  }

  fn unary(&mut self) -> Parsed<Expr> {
    self.skip();
    match self.peek() {
      Some(c @ (b'!' | b'~' | b'-' | b'+')) if !self.next_is("!=") => {
        self.at += 1;
        let operand = self.unary()?;
        Ok(Expr::Unary(c as char, Box::new(operand)))
      }
      _ => self.primary(),
    }
  }

  fn primary(&mut self) -> Parsed<Expr> {
    self.skip();
    let Some(c) = self.peek() else {
      return Err(self.expected("an expression"));
    };
    if c == b'(' {
      self.at += 1;
      let inner = self.expression()?;
      self.expect(")")?;
      return Ok(inner);
    }
    if c.is_ascii_digit() || (c == b'$' && self.peek_at(1).is_some_and(|c| c.is_ascii_hexdigit())) {
      return self.number();
    }
    if c == b'"' {
      let string = self.string().expect("a string");
      return Ok(Expr::String(string.text));
    }
    if self.eat_dot() {
      return Ok(Expr::Dot);
    }
    let Some(name) = self.name() else {
      return Err(self.expected("an expression"));
    };
    if self.next_is("(") {
      return self.call(name);
    }
    if !CONSTANTS.contains(&name.text.as_str()) {
      self.reference(name.clone(), RefKind::Symbol);
    }
    Ok(Expr::Symbol(name.text))
  }

  fn call(&mut self, name: Name) -> Parsed<Expr> {
    self.expect("(")?;
    let function = name.text.as_str();
    if !FUNCTIONS.contains(&function) {
      let mut message = format!("`{function}` isn't a function");
      let suggestion = suggest(function, FUNCTIONS);
      if let Some(suggestion) = &suggestion {
        message.push_str(&format!("; did you mean `{suggestion}`?"));
      }
      if self.script.errors.len() < MAX_ERRORS {
        self.script.errors.push(Problem {
          span: name.span.clone(),
          message,
          unknown: suggestion,
        });
      }
    }
    let mut arguments = Vec::new();
    let defined = function == "DEFINED";
    let was = self.in_defined;
    self.in_defined = was || defined;
    let result = (|| {
      if self.eat(")") {
        return Ok(());
      }
      loop {
        let kind = if SECTION_FUNCTIONS.contains(&function) {
          Some(RefKind::Section)
        } else if REGION_FUNCTIONS.contains(&function) {
          Some(RefKind::Region)
        } else {
          None
        };
        match kind {
          // A section or region by name.
          Some(kind) if self.peek_name().is_some() => {
            let name = self.name().expect("peeked");
            self.reference(name.clone(), kind);
            arguments.push(Expr::Symbol(name.text));
          }
          _ if function == "CONSTANT" => {
            let name = self
              .name()
              .ok_or_else(|| self.expected("MAXPAGESIZE or COMMONPAGESIZE"))?;
            arguments.push(Expr::Symbol(name.text));
          }
          _ if function == "SEGMENT_START" && arguments.is_empty() => {
            let segment = self
              .name_or_string()
              .ok_or_else(|| self.expected("a segment's name"))?;
            arguments.push(Expr::String(segment.text));
          }
          _ => arguments.push(self.expression()?),
        }
        if self.eat(")") {
          return Ok(());
        }
        self.expect(",")?;
      }
    })();
    self.in_defined = was;
    result?;
    Ok(Expr::Call(name.text, arguments))
  }

  fn number(&mut self) -> Parsed<Expr> {
    let start = self.at;
    if self.peek() == Some(b'$') {
      self.at += 1;
    }
    while self.peek().is_some_and(|c| c.is_ascii_alphanumeric()) {
      self.at += 1;
    }
    let text = &self.text[start..self.at];
    match number_value(text) {
      Some(value) => Ok(Expr::Number(value)),
      None => {
        self.error(start..self.at, format!("`{text}` isn't a number"));
        Ok(Expr::Invalid)
      }
    }
  }
}

fn is_sort(word: &str) -> bool {
  matches!(
    word,
    "SORT"
      | "SORT_BY_NAME"
      | "SORT_BY_ALIGNMENT"
      | "SORT_BY_INIT_PRIORITY"
      | "SORT_NONE"
      | "REVERSE"
  )
}

/// A number as ld reads it: `0x1000`, `$1000`, `1000h`, `0777` (octal),
/// `101b`, `4K`, `1M`.
pub fn number_value(text: &str) -> Option<u64> {
  // K and M aren't hex digits, so `0x10K` is scaled too.
  let (text, scale) = match text.as_bytes().last()? {
    b'K' | b'k' => (&text[..text.len() - 1], 1024),
    b'M' | b'm' => (&text[..text.len() - 1], 1024 * 1024),
    _ => (text, 1),
  };
  let value = if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
    u64::from_str_radix(hex, 16).ok()?
  } else if let Some(hex) = text.strip_prefix('$') {
    u64::from_str_radix(hex, 16).ok()?
  } else {
    let (digits, radix) = match text.as_bytes().last()? {
      b'h' | b'H' | b'x' | b'X' => (&text[..text.len() - 1], 16),
      b'o' | b'O' | b'q' | b'Q' => (&text[..text.len() - 1], 8),
      b'b' | b'B'
        if text[..text.len() - 1]
          .bytes()
          .all(|c| c == b'0' || c == b'1') =>
      {
        (&text[..text.len() - 1], 2)
      }
      b'd' | b'D' => (&text[..text.len() - 1], 10),
      _ if text.len() > 1 && text.starts_with('0') => (&text[1..], 8),
      _ => (text, 10),
    };
    if digits.is_empty() {
      return None;
    }
    u64::from_str_radix(digits, radix).ok()?
  };
  value.checked_mul(scale)
}

/// The closest of `known` to a mistyped `word`, if one is close.
pub fn suggest(word: &str, known: &[&str]) -> Option<String> {
  let upper = word.to_ascii_uppercase();
  let limit = (word.len() / 3).clamp(1, 3);
  known
    .iter()
    .map(|candidate| (distance(&upper, candidate), *candidate))
    .filter(|(distance, _)| *distance <= limit)
    .min_by_key(|(distance, _)| *distance)
    .map(|(_, candidate)| candidate.to_owned())
}

/// Edits from one word to another: characters added, removed, changed, or
/// two next to each other swapped.
pub fn distance(a: &str, b: &str) -> usize {
  let a: Vec<char> = a.chars().collect();
  let b: Vec<char> = b.chars().collect();
  let mut d = vec![vec![0usize; b.len() + 1]; a.len() + 1];
  for (i, row) in d.iter_mut().enumerate() {
    row[0] = i;
  }
  for (j, cell) in d[0].iter_mut().enumerate() {
    *cell = j;
  }
  for i in 1..=a.len() {
    for j in 1..=b.len() {
      let cost = usize::from(a[i - 1] != b[j - 1]);
      d[i][j] = (d[i - 1][j] + 1)
        .min(d[i][j - 1] + 1)
        .min(d[i - 1][j - 1] + cost);
      if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
        d[i][j] = d[i][j].min(d[i - 2][j - 2] + 1);
      }
    }
  }
  d[a.len()][b.len()]
}

/// The value of an expression, if it can be known without linking:
/// numbers, arithmetic, and `symbol`'s and `region`'s values.
pub fn evaluate(
  expr: &Expr,
  symbol: &dyn Fn(&str) -> Option<u64>,
  region: &dyn Fn(&str) -> Option<(u64, u64)>,
) -> Option<u64> {
  let eval = |expr: &Expr| evaluate(expr, symbol, region);
  Some(match expr {
    Expr::Number(value) => *value,
    Expr::Symbol(name) => symbol(name)?,
    Expr::Dot | Expr::String(_) | Expr::Invalid => return None,
    Expr::Unary(op, operand) => {
      let value = eval(operand)?;
      match op {
        '-' => value.wrapping_neg(),
        '~' => !value,
        '!' => u64::from(value == 0),
        _ => value,
      }
    }
    Expr::Binary(op, left, right) => {
      let (a, b) = (eval(left)?, eval(right)?);
      match *op {
        "+" => a.wrapping_add(b),
        "-" => a.wrapping_sub(b),
        "*" => a.wrapping_mul(b),
        "/" => a.checked_div(b)?,
        "%" => a.checked_rem(b)?,
        "<<" => a.checked_shl(u32::try_from(b).ok()?)?,
        ">>" => a.checked_shr(u32::try_from(b).ok()?)?,
        "&" => a & b,
        "|" => a | b,
        "^" => a ^ b,
        "&&" => u64::from(a != 0 && b != 0),
        "||" => u64::from(a != 0 || b != 0),
        "==" => u64::from(a == b),
        "!=" => u64::from(a != b),
        "<" => u64::from(a < b),
        ">" => u64::from(a > b),
        "<=" => u64::from(a <= b),
        ">=" => u64::from(a >= b),
        _ => return None,
      }
    }
    Expr::Conditional(condition, consequence, alternative) => {
      if eval(condition)? != 0 {
        eval(consequence)?
      } else {
        eval(alternative)?
      }
    }
    Expr::Call(function, arguments) => {
      let arg = |i: usize| arguments.get(i).and_then(eval);
      let name = |i: usize| match arguments.get(i) {
        Some(Expr::Symbol(name)) => Some(name.as_str()),
        _ => None,
      };
      match function.as_str() {
        "ORIGIN" => region(name(0)?)?.0,
        "LENGTH" => region(name(0)?)?.1,
        "MAX" => arg(0)?.max(arg(1)?),
        "MIN" => arg(0)?.min(arg(1)?),
        "ABSOLUTE" => arg(0)?,
        "LOG2CEIL" => {
          let value = arg(0)?;
          if value <= 1 {
            0
          } else {
            u64::from(64 - (value - 1).leading_zeros())
          }
        }
        "ALIGN" if arguments.len() == 2 => {
          let (value, align) = (arg(0)?, arg(1)?);
          if align == 0 {
            return None;
          }
          value.div_ceil(align).checked_mul(align)?
        }
        _ => return None,
      }
    }
  })
}

impl Script {
  /// The region with this name, or that an alias names.
  pub fn region(&self, name: &str) -> Option<&Region> {
    if let Some(region) = self.regions.iter().find(|region| region.name.text == name) {
      return Some(region);
    }
    let alias = self.aliases.iter().find(|alias| alias.name.text == name)?;
    self
      .regions
      .iter()
      .find(|region| region.name.text == alias.region.text)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn errors(text: &str) -> Vec<String> {
    parse(text)
      .errors
      .into_iter()
      .map(|problem| format!("{}: {}", &text[problem.span], problem.message))
      .collect()
  }

  const STM32: &str = include_str!("../samples/stm32.ld");
  const KERNEL: &str = include_str!("../samples/kernel.ld");
  const FLAREOS: &str = include_str!("../samples/flareos.ld");
  const GNU_ELF: &str = include_str!("../samples/gnu-ld-elf.ld");
  const GNU_I386: &str = include_str!("../samples/gnu-ld-i386.ld");
  const GNU_PE: &str = include_str!("../samples/gnu-ld-pe.ld");

  #[test]
  fn real_scripts_have_no_errors() {
    for (name, text) in [
      ("stm32", STM32),
      ("kernel", KERNEL),
      ("flareos", FLAREOS),
      ("gnu elf", GNU_ELF),
      ("gnu i386", GNU_I386),
      ("gnu pe", GNU_PE),
    ] {
      assert_eq!(errors(text), Vec::<String>::new(), "{name}");
    }
  }

  #[test]
  fn finds_what_a_script_defines() {
    let script = parse(STM32);
    let regions: Vec<_> = script
      .regions
      .iter()
      .map(|r| r.name.text.as_str())
      .collect();
    assert_eq!(regions, ["RAM", "FLASH"]);
    assert_eq!(script.regions[0].attributes, "xrw");
    let sections: Vec<_> = script
      .sections
      .iter()
      .map(|s| s.name.text.as_str())
      .collect();
    assert!(
      sections.starts_with(&[".isr_vector", ".text", ".ARM.extab"]),
      "{sections:?}"
    );
    let data = script
      .sections
      .iter()
      .find(|s| s.name.text == ".data")
      .expect("data");
    assert_eq!(data.region.as_ref().map(|r| r.text.as_str()), Some("RAM"));
    assert_eq!(
      data.load_region.as_ref().map(|r| r.text.as_str()),
      Some("FLASH")
    );
    let etext = script
      .symbols
      .iter()
      .find(|s| s.name.text == "_etext")
      .expect("_etext");
    assert_eq!(etext.section.as_deref(), Some(".text"));
    assert_eq!(etext.source, "_etext = .;");
    let end = script
      .symbols
      .iter()
      .find(|s| s.name.text == "end")
      .expect("end");
    assert_eq!(end.kind, SymbolKind::Provided);
    assert_eq!(
      script.entry.as_ref().map(|e| e.text.as_str()),
      Some("Reset_Handler")
    );
    assert_eq!(script.asserts[0].message, "RAM overflow");
    let discard = script.sections.iter().find(|s| s.discard).expect("discard");
    assert_eq!(discard.name.text, "/DISCARD/");
  }

  #[test]
  fn evaluates_regions() {
    let script = parse(STM32);
    let none = |_: &str| None;
    let region = |name: &str| {
      let region = script.region(name)?;
      Some((
        evaluate(&region.origin, &none, &|_| None)?,
        evaluate(&region.length, &none, &|_| None)?,
      ))
    };
    assert_eq!(region("FLASH"), Some((0x0800_0000, 64 * 1024)));
    assert_eq!(region("RAM"), Some((0x2000_0000, 20 * 1024)));
    let estack = script
      .symbols
      .iter()
      .find(|s| s.name.text == "_estack")
      .expect("_estack");
    assert_eq!(
      evaluate(&estack.value, &none, &region),
      Some(0x2000_0000 + 20 * 1024)
    );
  }

  #[test]
  fn reads_numbers_as_ld_does() {
    assert_eq!(number_value("0x1000"), Some(0x1000));
    assert_eq!(number_value("0X10K"), Some(0x10 * 1024));
    assert_eq!(number_value("$ff"), Some(255));
    assert_eq!(number_value("1000h"), Some(0x1000));
    assert_eq!(number_value("017"), Some(15));
    assert_eq!(number_value("101b"), Some(5));
    assert_eq!(number_value("4K"), Some(4096));
    assert_eq!(number_value("2M"), Some(2 * 1024 * 1024));
    assert_eq!(number_value("0"), Some(0));
    assert_eq!(number_value("0xAB"), Some(0xAB));
    assert_eq!(number_value("09"), None);
    assert_eq!(number_value("12z"), None);
  }

  #[test]
  fn references_know_what_they_name() {
    let script = parse(
      "SECTIONS { .text : { *(.text) } > ROM :text\n x = SIZEOF(.text) + ORIGIN(RAM) + y;\n z = DEFINED(w) ? w : 0; }",
    );
    let refs: Vec<_> = script
      .references
      .iter()
      .map(|r| (r.name.text.as_str(), r.kind, r.in_defined))
      .collect();
    assert_eq!(
      refs,
      [
        ("ROM", RefKind::Region, false),
        ("text", RefKind::Phdr, false),
        (".text", RefKind::Section, false),
        ("RAM", RefKind::Region, false),
        ("y", RefKind::Symbol, false),
        ("w", RefKind::Symbol, true),
        ("w", RefKind::Symbol, false),
      ]
    );
  }

  #[test]
  fn reports_syntax_errors_and_goes_on() {
    assert_eq!(
      errors("ENTRYY(_start)\nSECTIONS { .text : { *(.text) } }"),
      ["ENTRYY: `ENTRYY` isn't a command here; did you mean `ENTRY`?"]
    );
    assert_eq!(
      errors("x = 1\ny = 2;"),
      ["y: expected `;` after the assignment, found `y`"]
    );
    assert_eq!(
      errors("SECTIONS {\n  .text : { *(.text) \n}\n"),
      ["\n: expected `}` closing SECTIONS, found the end of the file"]
    );
    assert_eq!(
      errors("SECTIONS { .text : { KEPP(*(.init)) } }"),
      ["KEPP: `KEPP` isn't a command here; did you mean `KEEP`?"]
    );
    assert_eq!(
      errors("SECTIONS { . = ALIGNN(4); }"),
      ["ALIGNN: `ALIGNN` isn't a function; did you mean `ALIGN`?"]
    );
    assert_eq!(
      errors("MEMORY { RAM (rqx) : ORIGIN = 0, LENGTH = 1K\n ROM : org = 0, len = 4K }"),
      ["rqx: `q` isn't a region attribute (r, w, x, a, i, l, and ! for not)"]
    );
    assert_eq!(
      errors("MEMORY { RAM : START = 0, LENGTH = 1K\n ROM : o = 0, l = 4K }"),
      ["START: expected ORIGIN (or org, or o), found `START`"]
    );
    assert_eq!(errors("/* open"), ["/*: this comment isn't closed with */"]);
    assert_eq!(errors("x = 09;"), ["09: `09` isn't a number"]);
    // Good statements after a bad one are still read.
    let script = parse("SECTIONS { .a : { BAD BAD } .b : { *(.b) } }");
    assert!(script.sections.iter().any(|s| s.name.text == ".b"));
  }

  #[test]
  fn reads_tricky_statements() {
    for text in [
      "SECTIONS { .text 0x1000 (NOLOAD) : AT(0x2000) ALIGN(16) SUBALIGN(4) ONLY_IF_RO { *(.text) } > RAM AT> ROM :a :b = 0x90, }",
      "SECTIONS { .t (TYPE = SHT_PROGBITS) : { *(.t) } .r (READONLY) : { *(.r) } }",
      "SECTIONS { .ctors : { KEEP (*crtbegin.o(.ctors)) KEEP (*(EXCLUDE_FILE (*crtend.o) .ctors)) KEEP (*(SORT(.ctors.*))) } }",
      "SECTIONS { .x : { *(SORT_BY_ALIGNMENT(SORT_BY_NAME(.x.*))) libc.a:*(.data) [A-Z]*(.y) } }",
      "SECTIONS { .x : { INPUT_SECTION_FLAGS (SHF_WRITE) *(.x) BYTE(1) ASCIZ \"v1\" FILL(0x90) } }",
      "a-=1; b+=2; c = a - b; d = a-b;",
      "x = (1 << 4) >= 16 ? 1 : 0;",
      "SECTIONS { . = SEGMENT_START(\"text-segment\", 0x400000) + SIZEOF_HEADERS; }",
      "SECTIONS { . = DATA_SEGMENT_ALIGN (CONSTANT (MAXPAGESIZE), CONSTANT (COMMONPAGESIZE)); }",
      "INPUT(crt0.o -lc AS_NEEDED(libm.so)) GROUP(libc.a) SEARCH_DIR(\"=/usr/lib\");",
      "OUTPUT_FORMAT(\"elf32-littlearm\", \"elf32-bigarm\", \"elf32-littlearm\") OUTPUT_ARCH(arm)",
      "PHDRS { headers PT_PHDR PHDRS ; text PT_LOAD FILEHDR PHDRS FLAGS(5); }",
      "VERSION { V1 { global: f; local: *; }; }",
      "SECTIONS { OVERLAY 0x1000 : NOCROSSREFS AT(0x4000) { .a { *(.a) } .b { *(.b) } :p } > RAM }",
      "MEMORY { RAM(xrw) : o = 0x20000000, l = 20K\n ROM (!w): ORIGIN = 0, LENGTH = 1M }",
      "\"quoted sym\" = 1; PROVIDE(\"x\" = 2);",
      "# lld comments\nENTRY(main)",
    ] {
      assert_eq!(errors(text), Vec::<String>::new(), "{text}");
    }
  }

  #[test]
  fn suggests_close_names() {
    assert_eq!(suggest("ENTRYY", TOP_COMMANDS).as_deref(), Some("ENTRY"));
    assert_eq!(
      suggest("sections", TOP_COMMANDS).as_deref(),
      Some("SECTIONS")
    );
    assert_eq!(suggest("FOO", TOP_COMMANDS), None);
  }
}
