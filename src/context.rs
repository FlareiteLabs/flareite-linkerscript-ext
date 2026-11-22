//! What's expected where the cursor is, for completion: a command, a memory
//! region, a section, an expression, ... Found by scanning the text before
//! the cursor for its blocks, parentheses, and last tokens; the script
//! needn't parse.

use crate::parse::is_name_char;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Context {
  /// In a comment or a string: nothing.
  Nothing,
  /// A command at the top of the script.
  Top,
  /// An output section, an assignment, or a command, in SECTIONS.
  Sections,
  /// An input section, an assignment, or a command, in an output section.
  Body,
  /// After an output section's `:`: AT, ALIGN, SUBALIGN, ...
  Attribute,
  /// A section's type, in `( )` after its name.
  SectionType,
  /// In MEMORY: after a region's `:` (ORIGIN) or `,` (LENGTH).
  MemoryOrigin,
  MemoryLength,
  /// In MEMORY, where a region's name goes.
  MemoryName,
  /// A memory region: after `>`, `AT>`, or in ORIGIN( ) and LENGTH( ).
  Region,
  /// An output section: in ADDR( ), SIZEOF( ), INSERT AFTER, ...
  Section,
  /// A program header: after a section's `}` and `:`.
  Phdr,
  /// In PHDRS: a header's type after its name, or what follows.
  PhdrType,
  PhdrAttribute,
  /// A symbol: in ENTRY( ), DEFINED( ), PROVIDE( before `=`, ...
  Symbol,
  /// An expression: after `=`, an operator, or in a function's `( )`.
  Expression,
  /// In CONSTANT( ).
  Constant,
  /// The sections of an input section, in `*( )`.
  InputSection,
  /// A file name: after INCLUDE, in INPUT( ), ...
  File,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Block {
  Top,
  Sections,
  Body,
  Memory,
  Phdrs,
  Overlay,
  /// VERSION and anything else.
  Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
  Word(String),
  Punct(char),
  /// `>=`, `<<`, `+=`, and the like.
  Operator(String),
  Number,
  String,
}

/// A parenthesis open before the cursor, and the word before it.
struct Paren {
  word: Option<String>,
  /// Commas seen inside.
  commas: usize,
  /// Whether `=` was seen inside (PROVIDE(name = ...)).
  assigned: bool,
}

/// The context at `offset`, and where the word being typed there starts.
pub fn at(text: &str, offset: usize) -> (Context, usize) {
  let offset = offset.min(text.len());
  let bytes = text.as_bytes();
  // The word being typed.
  let mut start = offset;
  while start > 0
    && (is_name_char(bytes[start - 1]) || matches!(bytes[start - 1], b'*' | b'?' | b'/'))
  {
    start -= 1;
  }
  let Some(tokens) = tokens(&text[..start]) else {
    return (Context::Nothing, start);
  };
  (context(&tokens), start)
}

/// Whether `offset` is in a comment or a string.
pub fn in_comment_or_string(text: &str, offset: usize) -> bool {
  tokens(&text[..offset.min(text.len())]).is_none()
}

/// The tokens of `text`; `None` if it ends in a comment or a string.
fn tokens(text: &str) -> Option<Vec<Token>> {
  let bytes = text.as_bytes();
  let mut tokens = Vec::new();
  let mut i = 0;
  while i < bytes.len() {
    let c = bytes[i];
    if c.is_ascii_whitespace() {
      i += 1;
    } else if text[i..].starts_with("/*") {
      i = i + 2 + text[i + 2..].find("*/")? + 2;
    } else if c == b'#' {
      i += text[i..].find('\n')?;
    } else if c == b'"' {
      let end = text[i + 1..].find(['"', '\n'])?;
      if bytes[i + 1 + end] == b'\n' {
        return None;
      }
      i = i + 1 + end + 1;
      tokens.push(Token::String);
    } else if c.is_ascii_digit() {
      while i < bytes.len() && bytes[i].is_ascii_alphanumeric() {
        i += 1;
      }
      tokens.push(Token::Number);
    } else if is_name_char(c) || matches!(c, b'*' | b'?' | b'[' | b']') {
      let from = i;
      while i < bytes.len()
        && (is_name_char(bytes[i]) || matches!(bytes[i], b'*' | b'?' | b'[' | b']' | b':'))
      {
        // `:` joins archive:member patterns, not `name :`.
        if bytes[i] == b':'
          && !bytes
            .get(i + 1)
            .is_some_and(|&c| is_name_char(c) || c == b'*')
        {
          break;
        }
        i += 1;
      }
      tokens.push(Token::Word(text[from..i].to_owned()));
    } else {
      let rest = &text[i..];
      let operator = [
        "<<=", ">>=", "<<", ">>", "<=", ">=", "==", "!=", "&&", "||", "+=", "-=", "*=", "/=", "&=",
        "|=", "^=",
      ]
      .iter()
      .find(|op| rest.starts_with(**op));
      match operator {
        Some(op) => {
          tokens.push(Token::Operator((*op).to_owned()));
          i += op.len();
        }
        None => {
          let c = rest.chars().next().expect("not at the end");
          tokens.push(Token::Punct(c));
          i += c.len_utf8();
        }
      }
    }
  }
  Some(tokens)
}

fn word(token: Option<&Token>) -> Option<&str> {
  match token {
    Some(Token::Word(word)) => Some(word),
    _ => None,
  }
}

fn context(tokens: &[Token]) -> Context {
  let mut blocks = vec![Block::Top];
  let mut parens: Vec<Paren> = Vec::new();
  // Where the current statement's tokens start.
  let mut statement = 0;
  for (i, token) in tokens.iter().enumerate() {
    match token {
      Token::Punct('{') => {
        let first = word(tokens.get(statement));
        let block = match (*blocks.last().expect("top"), first) {
          (Block::Top, Some("SECTIONS")) => Block::Sections,
          (Block::Top, Some("MEMORY")) => Block::Memory,
          (Block::Top, Some("PHDRS")) => Block::Phdrs,
          (Block::Sections, Some("OVERLAY")) => Block::Overlay,
          (Block::Sections | Block::Overlay, _) => Block::Body,
          _ => Block::Other,
        };
        blocks.push(block);
        parens.clear();
        statement = i + 1;
      }
      Token::Punct('}') => {
        if blocks.len() > 1 {
          blocks.pop();
        }
        parens.clear();
        statement = i + 1;
      }
      Token::Punct('(') => {
        parens.push(Paren {
          word: word(i.checked_sub(1).and_then(|j| tokens.get(j))).map(str::to_owned),
          commas: 0,
          assigned: false,
        });
      }
      Token::Punct(')') => {
        parens.pop();
        // A command or input section ends with its `)`.
        if parens.is_empty() && matches!(blocks.last(), Some(Block::Top | Block::Body)) {
          statement = i + 1;
        }
      }
      Token::Punct(',') => {
        if let Some(paren) = parens.last_mut() {
          paren.commas += 1;
        } else if matches!(
          blocks.last(),
          Some(Block::Top | Block::Sections | Block::Body)
        ) {
          statement = i + 1;
        }
      }
      Token::Punct('=') => {
        if let Some(paren) = parens.last_mut() {
          paren.assigned = true;
        }
      }
      Token::Punct(';') if parens.is_empty() => statement = i + 1,
      _ => {}
    }
  }
  let block = *blocks.last().expect("top");
  let current = &tokens[statement.min(tokens.len())..];
  let last = tokens.last();
  let before_last = tokens.len().checked_sub(2).and_then(|i| tokens.get(i));

  // In parentheses.
  if let Some(paren) = parens.last() {
    let function = paren.word.as_deref().unwrap_or("");
    return match function {
      "ORIGIN" | "LENGTH" => Context::Region,
      "ADDR" | "SIZEOF" | "LOADADDR" | "ALIGNOF" | "NOCROSSREFS" | "NOCROSSREFS_TO" => {
        Context::Section
      }
      "CONSTANT" => Context::Constant,
      "DEFINED" | "ENTRY" | "EXTERN" => Context::Symbol,
      "PROVIDE" | "PROVIDE_HIDDEN" | "HIDDEN" if !paren.assigned => Context::Symbol,
      "REGION_ALIAS" if paren.commas > 0 => Context::Region,
      "REGION_ALIAS" => Context::Nothing,
      "INPUT" | "GROUP" | "AS_NEEDED" | "OUTPUT" | "SEARCH_DIR" | "STARTUP" => Context::File,
      "OUTPUT_FORMAT" | "OUTPUT_ARCH" | "TARGET" | "LD_FEATURE" => Context::Nothing,
      "KEEP" => Context::Body,
      "SORT"
      | "SORT_BY_NAME"
      | "SORT_BY_ALIGNMENT"
      | "SORT_BY_INIT_PRIORITY"
      | "SORT_NONE"
      | "REVERSE"
      | "EXCLUDE_FILE" => Context::InputSection,
      "INPUT_SECTION_FLAGS" => Context::Nothing,
      "FLAGS" | "AT" | "ALIGN" | "SUBALIGN" | "BYTE" | "SHORT" | "LONG" | "QUAD" | "SQUAD"
      | "FILL" | "ASSERT" => Context::Expression,
      _ if crate::parse::FUNCTIONS.contains(&function) => Context::Expression,
      // After an output section's name: its type.
      _ if block == Block::Sections && current.len() == 2 => Context::SectionType,
      // `*(`, `foo.o(`: an input section's sections.
      _ if block == Block::Body => Context::InputSection,
      _ => Context::Expression,
    };
  }

  // After an operator or `=`: an expression.
  let after_operator = matches!(
    last,
    Some(
      Token::Operator(_)
        | Token::Punct('=' | '+' | '-' | '*' | '/' | '%' | '&' | '|' | '^' | '!' | '~' | '?' | '<')
    )
  );
  if after_operator && block != Block::Memory {
    return Context::Expression;
  }

  match block {
    Block::Top => {
      if current.is_empty() {
        return Context::Top;
      }
      match word(current.first()) {
        Some("INCLUDE") if current.len() == 1 => Context::File,
        Some("INSERT") if current.len() == 2 => Context::Section,
        Some("INSERT") if current.len() == 1 => Context::Nothing,
        _ if matches!(last, Some(Token::Punct('>'))) => Context::Expression,
        // A new statement after a complete one on the line before.
        _ => Context::Nothing,
      }
    }
    Block::Sections | Block::Overlay => {
      // After a section's `}`: `> region`, `AT> region`, `:phdr`.
      if matches!(last, Some(Token::Punct('>'))) {
        return Context::Region;
      }
      if matches!(last, Some(Token::Punct(':'))) {
        let closed = current.is_empty()
          || matches!(before_last, Some(Token::Punct('}')))
          || is_after_section(tokens);
        if closed {
          return Context::Phdr;
        }
        return Context::Attribute;
      }
      if current.is_empty() {
        return Context::Sections;
      }
      // After a section's `:`, its attributes until `{`.
      if current
        .iter()
        .any(|token| matches!(token, Token::Punct(':')))
        && matches!(last, Some(Token::Punct(')') | Token::Word(_)))
      {
        return Context::Attribute;
      }
      if word(current.first()) == Some("INCLUDE") && current.len() == 1 {
        return Context::File;
      }
      // After a section and its `> region`, `:phdr`, or `= fill`.
      if matches!(last, Some(Token::Punct('}')))
        || (matches!(last, Some(Token::Word(_) | Token::Number)) && is_after_section(tokens))
      {
        return Context::Sections;
      }
      // `.text` then a space: perhaps its address, type, or `:`.
      Context::Nothing
    }
    Block::Body => {
      if matches!(last, Some(Token::Punct('>'))) {
        return Context::Region;
      }
      if current.is_empty() || matches!(last, Some(Token::Word(_)) | Some(Token::Punct(')'))) {
        if word(current.first()) == Some("INCLUDE") && current.len() == 1 {
          return Context::File;
        }
        return Context::Body;
      }
      Context::Nothing
    }
    Block::Memory => {
      match last {
        Some(Token::Punct(':')) => Context::MemoryOrigin,
        Some(Token::Punct(',')) => Context::MemoryLength,
        Some(Token::Punct('=')) => Context::Expression,
        // After an origin's value: the length (its `,` is optional).
        Some(Token::Number | Token::Punct(')')) => {
          if current_region_has(tokens, &["LENGTH", "len", "l"]) {
            Context::MemoryName
          } else {
            Context::MemoryLength
          }
        }
        Some(Token::Word(_)) if matches!(before_last, Some(Token::Punct('='))) => {
          if current_region_has(tokens, &["LENGTH", "len", "l"]) {
            Context::MemoryName
          } else {
            Context::MemoryLength
          }
        }
        _ => Context::MemoryName,
      }
    }
    Block::Phdrs => {
      let since: Vec<&Token> = tokens
        .iter()
        .rev()
        .take_while(|token| !matches!(token, Token::Punct(';' | '{')))
        .collect();
      match since.len() {
        0 => Context::Nothing,
        1 => Context::PhdrType,
        _ => Context::PhdrAttribute,
      }
    }
    Block::Other => Context::Nothing,
  }
}

/// Whether the tokens end with a section's `}` and what may follow it
/// (`> region`, `AT> region`, `:phdr`, `= fill`), and one more token.
fn is_after_section(tokens: &[Token]) -> bool {
  let mut i = tokens.len().saturating_sub(1);
  // Skip the last `:`.
  loop {
    if i == 0 {
      return false;
    }
    i -= 1;
    match &tokens[i] {
      Token::Punct('}') => return true,
      Token::Word(_) | Token::Number | Token::Punct('>' | ':' | '=') => {}
      _ => return false,
    }
  }
}

/// Whether the region being declared already has one of `words`.
fn current_region_has(tokens: &[Token], words: &[&str]) -> bool {
  for token in tokens.iter().rev() {
    match token {
      Token::Punct(':' | '{') => return false,
      Token::Word(word) if words.contains(&word.as_str()) => return true,
      _ => {}
    }
  }
  false
}

#[cfg(test)]
mod tests {
  use super::*;

  /// The context at `|` in `text`.
  fn at_bar(text: &str) -> Context {
    let offset = text.find('|').expect("a cursor");
    let text = text.replace('|', "");
    at(&text, offset).0
  }

  #[test]
  fn knows_blocks() {
    assert_eq!(at_bar("|"), Context::Top);
    assert_eq!(at_bar("ENTRY(_start)\n|"), Context::Top);
    assert_eq!(at_bar("x = 1;\nSEC|"), Context::Top);
    assert_eq!(at_bar("SECTIONS {\n  |"), Context::Sections);
    assert_eq!(
      at_bar("SECTIONS {\n  .text : { *(.text) } > RAM\n  .da|"),
      Context::Sections
    );
    assert_eq!(at_bar("SECTIONS {\n  .text : {\n    |"), Context::Body);
    assert_eq!(
      at_bar("SECTIONS {\n  .text : {\n    *(.text)\n    KE|"),
      Context::Body
    );
    assert_eq!(at_bar("SECTIONS { .text : { KEEP(|"), Context::Body);
    assert_eq!(at_bar("SECTIONS { .text : { *(|"), Context::InputSection);
    assert_eq!(
      at_bar("SECTIONS { .text : { *(.text .te|"),
      Context::InputSection
    );
    assert_eq!(
      at_bar("SECTIONS { .text : { *(SORT(|"),
      Context::InputSection
    );
    assert_eq!(at_bar("/* SECTIONS { |"), Context::Nothing);
    assert_eq!(at_bar("x = \"a |"), Context::Nothing);
  }

  #[test]
  fn knows_names() {
    assert_eq!(at_bar("SECTIONS { .text : { } > |"), Context::Region);
    assert_eq!(at_bar("SECTIONS { .text : { } >FL|"), Context::Region);
    assert_eq!(
      at_bar("SECTIONS { .data : { } > RAM AT> |"),
      Context::Region
    );
    assert_eq!(at_bar("SECTIONS { .text : { } :|"), Context::Phdr);
    assert_eq!(at_bar("SECTIONS { .text : { } > RAM :|"), Context::Phdr);
    assert_eq!(at_bar("x = ORIGIN(|"), Context::Region);
    assert_eq!(at_bar("x = SIZEOF(.t|"), Context::Section);
    assert_eq!(at_bar("x = DEFINED(|"), Context::Symbol);
    assert_eq!(at_bar("ENTRY(|"), Context::Symbol);
    assert_eq!(at_bar("PROVIDE(|"), Context::Symbol);
    assert_eq!(at_bar("PROVIDE(end = |"), Context::Expression);
    assert_eq!(at_bar("x = CONSTANT(|"), Context::Constant);
    assert_eq!(at_bar("REGION_ALIAS(\"A\", |"), Context::Region);
    assert_eq!(at_bar("INCLUDE |"), Context::File);
    assert_eq!(at_bar("INSERT AFTER |"), Context::Section);
  }

  #[test]
  fn knows_expressions_and_attributes() {
    assert_eq!(at_bar("x = |"), Context::Expression);
    assert_eq!(at_bar("x = a + |"), Context::Expression);
    assert_eq!(at_bar("SECTIONS { . = ALIGN(|"), Context::Expression);
    assert_eq!(at_bar("SECTIONS { .text : { . = |"), Context::Expression);
    assert_eq!(at_bar("SECTIONS { .text : |"), Context::Attribute);
    assert_eq!(at_bar("SECTIONS { .text : AL|"), Context::Attribute);
    assert_eq!(at_bar("SECTIONS { .text : AT(0x100) |"), Context::Attribute);
    assert_eq!(at_bar("SECTIONS { .bss (|"), Context::SectionType);
    assert_eq!(at_bar("SECTIONS { .text : { BYTE(|"), Context::Expression);
  }

  #[test]
  fn knows_memory_and_phdrs() {
    assert_eq!(at_bar("MEMORY {\n  |"), Context::MemoryName);
    assert_eq!(at_bar("MEMORY {\n  RAM (rwx) : |"), Context::MemoryOrigin);
    assert_eq!(
      at_bar("MEMORY {\n  RAM (rwx) : ORIGIN = 0x0, |"),
      Context::MemoryLength
    );
    assert_eq!(
      at_bar("MEMORY {\n  RAM (rwx) : ORIGIN = 0x0 |"),
      Context::MemoryLength
    );
    assert_eq!(
      at_bar("MEMORY {\n  RAM : ORIGIN = 0, LENGTH = 4K\n  |"),
      Context::MemoryName
    );
    assert_eq!(at_bar("MEMORY {\n  RAM : ORIGIN = |"), Context::Expression);
    assert_eq!(at_bar("PHDRS {\n  text |"), Context::PhdrType);
    assert_eq!(at_bar("PHDRS {\n  text PT_LOAD |"), Context::PhdrAttribute);
  }
}
