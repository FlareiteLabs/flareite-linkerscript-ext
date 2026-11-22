//! Reading a linked ELF program: its segments, sections, and symbols, for
//! the Memory Layout view. 32- and 64-bit, either byte order.

/// Largest symbol or string table read.
const MAX_TABLE_BYTES: u64 = 16 * 1024 * 1024;
/// Most symbols kept.
const MAX_SYMBOLS: usize = 200_000;

pub const PT_LOAD: u32 = 1;
pub const SHT_NOBITS: u32 = 8;
const SHT_SYMTAB: u32 = 2;
pub const SHF_WRITE: u64 = 1;
pub const SHF_ALLOC: u64 = 2;
pub const SHF_EXECINSTR: u64 = 4;
pub const SHF_TLS: u64 = 0x400;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
  pub kind: u32,
  pub flags: u32,
  pub offset: u64,
  pub vaddr: u64,
  pub paddr: u64,
  pub filesz: u64,
  pub memsz: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
  pub index: usize,
  pub name: String,
  pub kind: u32,
  pub flags: u64,
  pub addr: u64,
  pub offset: u64,
  pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
  pub name: String,
  pub value: u64,
  pub size: u64,
  /// The index of its section.
  pub section: usize,
  /// STT_FUNC (2) is code; the rest data.
  pub kind: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Elf {
  pub bits: u8,
  pub little: bool,
  pub machine: u16,
  pub entry: u64,
  pub segments: Vec<Segment>,
  pub sections: Vec<Section>,
  pub symbols: Vec<Symbol>,
}

/// Reads `length` bytes at `offset`, fewer at the end of the file.
pub type Read<'a> = dyn FnMut(u64, u32) -> Result<Vec<u8>, String> + 'a;

struct Bytes<'a> {
  data: &'a [u8],
  little: bool,
}

impl Bytes<'_> {
  fn u16(&self, at: usize) -> Option<u16> {
    let b: [u8; 2] = self.data.get(at..at + 2)?.try_into().ok()?;
    Some(if self.little {
      u16::from_le_bytes(b)
    } else {
      u16::from_be_bytes(b)
    })
  }

  fn u32(&self, at: usize) -> Option<u32> {
    let b: [u8; 4] = self.data.get(at..at + 4)?.try_into().ok()?;
    Some(if self.little {
      u32::from_le_bytes(b)
    } else {
      u32::from_be_bytes(b)
    })
  }

  fn u64(&self, at: usize) -> Option<u64> {
    let b: [u8; 8] = self.data.get(at..at + 8)?.try_into().ok()?;
    Some(if self.little {
      u64::from_le_bytes(b)
    } else {
      u64::from_be_bytes(b)
    })
  }

  /// A word: 8 bytes in a 64-bit file, 4 in a 32-bit one.
  fn word(&self, at: usize, wide: bool) -> Option<u64> {
    if wide {
      self.u64(at)
    } else {
      self.u32(at).map(u64::from)
    }
  }
}

fn read_exact(read: &mut Read, offset: u64, length: u64, what: &str) -> Result<Vec<u8>, String> {
  if length > MAX_TABLE_BYTES {
    return Err(format!("its {what} is too large to read"));
  }
  let bytes = read(offset, length as u32)?;
  if (bytes.len() as u64) < length {
    return Err(format!("it ends before its {what}"));
  }
  Ok(bytes)
}

/// Reads an ELF file.
pub fn parse(read: &mut Read) -> Result<Elf, String> {
  let header = read(0, 64)?;
  if header.len() < 52 || &header[..4] != b"\x7fELF" {
    return Err("it isn't an ELF file".into());
  }
  let wide = match header[4] {
    1 => false,
    2 => true,
    _ => return Err("its ELF class is unknown".into()),
  };
  let little = match header[5] {
    1 => true,
    2 => false,
    _ => return Err("its byte order is unknown".into()),
  };
  let h = Bytes {
    data: &header,
    little,
  };
  let bad = || "its header is cut short".to_owned();
  let machine = h.u16(18).ok_or_else(bad)?;
  let (entry, phoff, shoff, rest) = if wide {
    (h.u64(24), h.u64(32), h.u64(40), 48)
  } else {
    (
      h.u32(24).map(u64::from),
      h.u32(28).map(u64::from),
      h.u32(32).map(u64::from),
      36,
    )
  };
  let (entry, phoff, shoff) = (
    entry.ok_or_else(bad)?,
    phoff.ok_or_else(bad)?,
    shoff.ok_or_else(bad)?,
  );
  let phentsize = u64::from(h.u16(rest + 6).ok_or_else(bad)?);
  let phnum = u64::from(h.u16(rest + 8).ok_or_else(bad)?);
  let shentsize = u64::from(h.u16(rest + 10).ok_or_else(bad)?);
  let shnum = u64::from(h.u16(rest + 12).ok_or_else(bad)?);
  let shstrndx = usize::from(h.u16(rest + 14).ok_or_else(bad)?);

  let mut segments = Vec::new();
  if phnum > 0 && phoff > 0 {
    let table = read_exact(read, phoff, phentsize * phnum, "program headers")?;
    let t = Bytes {
      data: &table,
      little,
    };
    for i in 0..phnum as usize {
      let at = i * phentsize as usize;
      let segment = if wide {
        Segment {
          kind: t.u32(at).ok_or_else(bad)?,
          flags: t.u32(at + 4).ok_or_else(bad)?,
          offset: t.u64(at + 8).ok_or_else(bad)?,
          vaddr: t.u64(at + 16).ok_or_else(bad)?,
          paddr: t.u64(at + 24).ok_or_else(bad)?,
          filesz: t.u64(at + 32).ok_or_else(bad)?,
          memsz: t.u64(at + 40).ok_or_else(bad)?,
        }
      } else {
        Segment {
          kind: t.u32(at).ok_or_else(bad)?,
          offset: u64::from(t.u32(at + 4).ok_or_else(bad)?),
          vaddr: u64::from(t.u32(at + 8).ok_or_else(bad)?),
          paddr: u64::from(t.u32(at + 12).ok_or_else(bad)?),
          filesz: u64::from(t.u32(at + 16).ok_or_else(bad)?),
          memsz: u64::from(t.u32(at + 20).ok_or_else(bad)?),
          flags: t.u32(at + 24).ok_or_else(bad)?,
        }
      };
      segments.push(segment);
    }
  }

  // Section headers: name, type, flags, address, offset, size, link.
  let mut raw = Vec::new();
  if shnum > 0 && shoff > 0 {
    let table = read_exact(read, shoff, shentsize * shnum, "section headers")?;
    let t = Bytes {
      data: &table,
      little,
    };
    for i in 0..shnum as usize {
      let at = i * shentsize as usize;
      let name = t.u32(at).ok_or_else(bad)?;
      let kind = t.u32(at + 4).ok_or_else(bad)?;
      let w = if wide { 8 } else { 4 };
      let flags = t.word(at + 8, wide).ok_or_else(bad)?;
      let addr = t.word(at + 8 + w, wide).ok_or_else(bad)?;
      let offset = t.word(at + 8 + 2 * w, wide).ok_or_else(bad)?;
      let size = t.word(at + 8 + 3 * w, wide).ok_or_else(bad)?;
      let link = t.u32(at + 8 + 4 * w).ok_or_else(bad)?;
      raw.push((name, kind, flags, addr, offset, size, link));
    }
  }
  let names = match raw.get(shstrndx) {
    Some(&(_, _, _, _, offset, size, _)) if shstrndx != 0 => {
      read_exact(read, offset, size, "section names")?
    }
    _ => Vec::new(),
  };
  let sections: Vec<Section> = raw
    .iter()
    .enumerate()
    .map(
      |(index, &(name, kind, flags, addr, offset, size, _))| Section {
        index,
        name: string_at(&names, name as usize),
        kind,
        flags,
        addr,
        offset,
        size,
      },
    )
    .collect();

  let mut symbols = Vec::new();
  if let Some(&(_, _, _, _, offset, size, link)) =
    raw.iter().find(|section| section.1 == SHT_SYMTAB)
    && let Some(&(_, _, _, _, strings_offset, strings_size, _)) = raw.get(link as usize)
  {
    let table = read_exact(read, offset, size.min(MAX_TABLE_BYTES), "symbols")?;
    let strings = read_exact(
      read,
      strings_offset,
      strings_size.min(MAX_TABLE_BYTES),
      "symbol names",
    )?;
    let t = Bytes {
      data: &table,
      little,
    };
    let entry = if wide { 24 } else { 16 };
    for i in 1..table.len() / entry {
      let at = i * entry;
      let (name, info, shndx, value, size) = if wide {
        (
          t.u32(at),
          table.get(at + 4).copied(),
          t.u16(at + 6),
          t.u64(at + 8),
          t.u64(at + 16),
        )
      } else {
        (
          t.u32(at),
          table.get(at + 12).copied(),
          t.u16(at + 14),
          t.u32(at + 4).map(u64::from),
          t.u32(at + 8).map(u64::from),
        )
      };
      let (Some(name), Some(info), Some(shndx), Some(value), Some(size)) =
        (name, info, shndx, value, size)
      else {
        break;
      };
      let kind = info & 0xf;
      // Sized code and data in a section (not SHN_UNDEF or SHN_ABS).
      if size == 0 || shndx == 0 || shndx >= 0xff00 || !matches!(kind, 0..=2 | 6) {
        continue;
      }
      symbols.push(Symbol {
        name: demangle(&string_at(&strings, name as usize)),
        value,
        size,
        section: usize::from(shndx),
        kind,
      });
      if symbols.len() >= MAX_SYMBOLS {
        break;
      }
    }
  }

  Ok(Elf {
    bits: if wide { 64 } else { 32 },
    little,
    machine,
    entry,
    segments,
    sections,
    symbols,
  })
}

fn string_at(table: &[u8], at: usize) -> String {
  let Some(rest) = table.get(at..) else {
    return String::new();
  };
  let end = rest.iter().position(|&c| c == 0).unwrap_or(rest.len());
  String::from_utf8_lossy(&rest[..end]).into_owned()
}

impl Elf {
  /// What the machine is called.
  pub fn machine_name(&self) -> String {
    match self.machine {
      3 => "x86".into(),
      62 => "x86-64".into(),
      40 => "ARM".into(),
      183 => "AArch64".into(),
      243 => format!("RISC-V {}", self.bits),
      8 => "MIPS".into(),
      20 => "PowerPC".into(),
      21 => "PowerPC 64".into(),
      105 => "MSP430".into(),
      83 => "AVR".into(),
      258 => "LoongArch".into(),
      2 | 43 => "SPARC".into(),
      94 => "Xtensa".into(),
      other => format!("machine {other}"),
    }
  }

  /// The sections that take memory when the program runs.
  pub fn allocated(&self) -> impl Iterator<Item = &Section> {
    self
      .sections
      .iter()
      .filter(|section| section.flags & SHF_ALLOC != 0 && section.size > 0)
  }

  /// Where a section is loaded from (its LMA): its address moved as its
  /// segment's physical address is from its virtual one.
  pub fn load_address(&self, section: &Section) -> u64 {
    self
      .segments
      .iter()
      .filter(|segment| segment.kind == PT_LOAD)
      .find(|segment| {
        section.addr >= segment.vaddr
          && section.addr < segment.vaddr + segment.memsz.max(1)
          && (section.kind == SHT_NOBITS
            || (section.offset >= segment.offset
              && section.offset < segment.offset + segment.filesz.max(1)))
      })
      .map_or(section.addr, |segment| {
        section
          .addr
          .wrapping_sub(segment.vaddr)
          .wrapping_add(segment.paddr)
      })
  }

  /// The sections (by index) inside a segment.
  pub fn sections_in(&self, segment: &Segment) -> Vec<&Section> {
    self
      .allocated()
      .filter(|section| {
        section.addr >= segment.vaddr
          && section.addr + section.size <= segment.vaddr + segment.memsz
          && (section.kind == SHT_NOBITS || section.offset >= segment.offset)
      })
      .collect()
  }
}

/// A Rust symbol's name without its mangling, legacy or v0 (`_ZN6kernel4main17h…E`
/// and `_RNvCs…_6kernel4main` are both `kernel::main`); others as they are.
pub fn demangle(name: &str) -> String {
  match rustc_demangle::try_demangle(name) {
    // `#` leaves out the hash.
    Ok(demangled) => format!("{demangled:#}"),
    Err(_) => name.to_owned(),
  }
}

#[cfg(test)]
pub mod tests {
  use super::*;

  /// A little-endian ELF64 file with these sections (name, type, flags,
  /// address, size), one PT_LOAD per allocated section with its load
  /// address, and symbols (name, value, size, section index).
  pub fn build(
    sections: &[(&str, u32, u64, u64, u64, u64)],
    symbols: &[(&str, u64, u64, u16)],
  ) -> Vec<u8> {
    let mut file = vec![0u8; 64];
    // Section names.
    let mut names = vec![0u8];
    let mut name_offsets = Vec::new();
    for (name, ..) in sections {
      name_offsets.push(names.len() as u32);
      names.extend(name.as_bytes());
      names.push(0);
    }
    let shstrtab_name = names.len() as u32;
    names.extend(b".shstrtab\0");
    let symtab_name = names.len() as u32;
    names.extend(b".symtab\0");
    let strtab_name = names.len() as u32;
    names.extend(b".strtab\0");
    let names_offset = file.len() as u64;
    file.extend(&names);
    // Symbols.
    let mut strings = vec![0u8];
    let mut table = vec![0u8; 24];
    for (name, value, size, section) in symbols {
      let at = strings.len() as u32;
      strings.extend(name.as_bytes());
      strings.push(0);
      table.extend(at.to_le_bytes());
      table.push(2); // STT_FUNC
      table.push(0);
      table.extend(section.to_le_bytes());
      table.extend(value.to_le_bytes());
      table.extend(size.to_le_bytes());
    }
    let strings_offset = file.len() as u64;
    file.extend(&strings);
    let table_offset = file.len() as u64;
    file.extend(&table);
    // Program headers.
    let phoff = file.len() as u64;
    let loads: Vec<_> = sections.iter().filter(|s| s.2 & SHF_ALLOC != 0).collect();
    for (_, kind, flags, addr, size, lma) in &loads {
      file.extend(PT_LOAD.to_le_bytes());
      file.extend((if flags & SHF_EXECINSTR != 0 { 5u32 } else { 6 }).to_le_bytes());
      file.extend(0u64.to_le_bytes());
      file.extend(addr.to_le_bytes());
      file.extend(lma.to_le_bytes());
      file.extend((if *kind == SHT_NOBITS { 0 } else { *size }).to_le_bytes());
      file.extend(size.to_le_bytes());
      file.extend(0x1000u64.to_le_bytes());
    }
    // Section headers: null, the given, .shstrtab, .symtab, .strtab.
    let shoff = file.len() as u64;
    let mut header =
      |name: u32, kind: u32, flags: u64, addr: u64, offset: u64, size: u64, link: u32| {
        file.extend(name.to_le_bytes());
        file.extend(kind.to_le_bytes());
        file.extend(flags.to_le_bytes());
        file.extend(addr.to_le_bytes());
        file.extend(offset.to_le_bytes());
        file.extend(size.to_le_bytes());
        file.extend(link.to_le_bytes());
        file.extend([0u8; 20]);
      };
    header(0, 0, 0, 0, 0, 0, 0);
    for (i, (_, kind, flags, addr, size, _)) in sections.iter().enumerate() {
      header(name_offsets[i], *kind, *flags, *addr, 0, *size, 0);
    }
    let count = sections.len() as u32;
    header(shstrtab_name, 3, 0, 0, names_offset, names.len() as u64, 0);
    header(
      symtab_name,
      SHT_SYMTAB,
      0,
      0,
      table_offset,
      table.len() as u64,
      count + 3,
    );
    header(
      strtab_name,
      3,
      0,
      0,
      strings_offset,
      strings.len() as u64,
      0,
    );
    // The file header.
    file[..4].copy_from_slice(b"\x7fELF");
    file[4] = 2;
    file[5] = 1;
    file[6] = 1;
    file[16..18].copy_from_slice(&2u16.to_le_bytes());
    file[18..20].copy_from_slice(&40u16.to_le_bytes()); // ARM
    file[24..32].copy_from_slice(&0x0800_0000u64.to_le_bytes());
    file[32..40].copy_from_slice(&phoff.to_le_bytes());
    file[40..48].copy_from_slice(&shoff.to_le_bytes());
    file[52..54].copy_from_slice(&64u16.to_le_bytes());
    file[54..56].copy_from_slice(&56u16.to_le_bytes());
    file[56..58].copy_from_slice(&(loads.len() as u16).to_le_bytes());
    file[58..60].copy_from_slice(&64u16.to_le_bytes());
    file[60..62].copy_from_slice(&(count as u16 + 4).to_le_bytes());
    file[62..64].copy_from_slice(&(count as u16 + 1).to_le_bytes());
    file
  }

  pub fn reader(file: &[u8]) -> impl FnMut(u64, u32) -> Result<Vec<u8>, String> + '_ {
    move |offset, length| {
      let start = (offset as usize).min(file.len());
      let end = (start + length as usize).min(file.len());
      Ok(file[start..end].to_vec())
    }
  }

  #[test]
  fn reads_sections_segments_and_symbols() {
    let file = build(
      &[
        (
          ".text",
          1,
          SHF_ALLOC | SHF_EXECINSTR,
          0x0800_0000,
          0x400,
          0x0800_0000,
        ),
        (
          ".data",
          1,
          SHF_ALLOC | SHF_WRITE,
          0x2000_0000,
          0x40,
          0x0800_0400,
        ),
        (
          ".bss",
          SHT_NOBITS,
          SHF_ALLOC | SHF_WRITE,
          0x2000_0040,
          0x100,
          0x2000_0040,
        ),
        (".comment", 1, 0, 0, 0x20, 0),
      ],
      &[
        ("_ZN6kernel4main17h0123456789abcdefE", 0x0800_0100, 0x80, 1),
        ("buffer", 0x2000_0040, 0x100, 3),
        ("undefined", 0, 4, 0),
      ],
    );
    let elf = parse(&mut reader(&file)).expect("parses");
    assert_eq!(
      (elf.bits, elf.little, elf.machine_name()),
      (64, true, "ARM".into())
    );
    let names: Vec<_> = elf.allocated().map(|s| s.name.as_str()).collect();
    assert_eq!(names, [".text", ".data", ".bss"]);
    let data = &elf.sections[2];
    assert_eq!(elf.load_address(data), 0x0800_0400, "loaded in flash");
    assert_eq!(elf.load_address(&elf.sections[3]), 0x2000_0040);
    assert_eq!(elf.segments.len(), 3);
    assert_eq!(elf.sections_in(&elf.segments[0]).len(), 1);
    let symbols: Vec<_> = elf
      .symbols
      .iter()
      .map(|s| (s.name.as_str(), s.section))
      .collect();
    assert_eq!(symbols, [("kernel::main", 1), ("buffer", 3)]);
  }

  #[test]
  fn refuses_other_files() {
    let mut read = reader(b"MZ\x90\0");
    assert_eq!(parse(&mut read), Err("it isn't an ELF file".into()));
  }

  #[test]
  fn demangles_rust_names() {
    assert_eq!(
      demangle("_ZN4core3fmt5write17h0123456789abcdefE"),
      "core::fmt::write"
    );
    assert_eq!(
      demangle("_RNvCs8ruLb4KHyEa_14flareos_kernel5kmain"),
      "flareos_kernel::kmain"
    );
    assert_eq!(
      demangle("_RNvMNtCs8ruLb4KHyEa_14flareos_kernel3vgaNtB2_9VgaWriter8new_line"),
      "<flareos_kernel::vga::VgaWriter>::new_line"
    );
    assert_eq!(demangle("main"), "main");
  }
}
