//! The Memory Layout view's rows, from a linked program and the memory
//! regions the scripts declare: how full each region is and what's in it,
//! the program's segments, and its sections with their largest symbols.

use crate::elf::{Elf, PT_LOAD, SHF_EXECINSTR, SHF_TLS, SHF_WRITE, SHT_NOBITS, Section};

/// Most symbols listed under a section.
const MAX_SYMBOLS_SHOWN: usize = 30;
/// Usage from which a region is marked as nearly full.
const HIGH_PERCENT: u64 = 90;

/// A memory region with its values known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegionSpan {
  pub name: String,
  pub origin: u64,
  pub length: u64,
  pub attributes: String,
}

/// What a row does when clicked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
  ChooseProgram,
  /// Goes to where a script defines this region, section, or symbol.
  Region(String),
  Section(String),
  Symbol(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Icon {
  None,
  Named(&'static str),
  File(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
  pub id: String,
  pub parent: Option<String>,
  pub label: String,
  pub description: Option<String>,
  pub tooltip: Option<String>,
  pub icon: Icon,
  /// `high` or `full`: a color the manifest declares.
  pub color: Option<&'static str>,
  pub badge: Option<String>,
  pub expanded: Option<bool>,
  pub action: Option<Action>,
}

impl Row {
  fn new(id: impl Into<String>, parent: Option<&str>, label: impl Into<String>) -> Self {
    Self {
      id: id.into(),
      parent: parent.map(str::to_owned),
      label: label.into(),
      description: None,
      tooltip: None,
      icon: Icon::None,
      color: None,
      badge: None,
      expanded: None,
      action: None,
    }
  }
}

/// `1.5 KiB`, `12 B`, `2.0 MiB`.
pub fn size(bytes: u64) -> String {
  const KIB: f64 = 1024.0;
  let value = bytes as f64;
  if bytes < 1024 {
    format!("{bytes} B")
  } else if value < KIB * KIB {
    format!("{:.1} KiB", value / KIB)
  } else if value < KIB * KIB * KIB {
    format!("{:.1} MiB", value / KIB / KIB)
  } else {
    format!("{:.1} GiB", value / KIB / KIB / KIB)
  }
}

/// An address in hex, as wide as the program's addresses.
pub fn address(value: u64, bits: u8) -> String {
  if bits == 64 && value > u64::from(u32::MAX) {
    format!("0x{value:016x}")
  } else {
    format!("0x{value:08x}")
  }
}

fn flags(section: &Section) -> String {
  let mut text = String::from("R");
  if section.flags & SHF_WRITE != 0 {
    text.push('W');
  }
  if section.flags & SHF_EXECINSTR != 0 {
    text.push('X');
  }
  text
}

/// How much of a region is used, and by what.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Usage {
  pub used: u64,
  /// Sections running there, and those loaded there to be copied (their
  /// load images), with the address and size of each.
  pub parts: Vec<(String, u64, u64, bool)>,
}

pub fn usage(elf: &Elf, region: &RegionSpan) -> Usage {
  let end = region.origin.saturating_add(region.length);
  let inside = |address: u64| address >= region.origin && address < end;
  let mut parts = Vec::new();
  let mut used = 0u64;
  for section in elf.allocated() {
    // Thread-local .tbss takes no memory of its own.
    if section.kind == SHT_NOBITS && section.flags & SHF_TLS != 0 {
      continue;
    }
    if inside(section.addr) {
      used += section.size;
      parts.push((section.name.clone(), section.addr, section.size, false));
    }
    let load = elf.load_address(section);
    if load != section.addr && section.kind != SHT_NOBITS && inside(load) {
      used += section.size;
      parts.push((section.name.clone(), load, section.size, true));
    }
  }
  parts.sort_by_key(|part| part.1);
  Usage { used, parts }
}

fn percent(used: u64, length: u64) -> u64 {
  if length == 0 {
    return 0;
  }
  (u128::from(used) * 100 / u128::from(length)) as u64
}

/// The view's rows for `elf` (read from `path`, shown as `shown`).
pub fn rows(elf: &Elf, path: &str, shown: &str, regions: &[RegionSpan]) -> Vec<Row> {
  let mut rows = Vec::new();
  let name = shown.rsplit(['/', '\\']).next().unwrap_or(shown);
  let mut program = Row::new("program", None, name);
  program.description = Some(format!(
    "ELF{} · {} · entry {}",
    elf.bits,
    elf.machine_name(),
    address(elf.entry, elf.bits)
  ));
  program.tooltip = Some(format!("{path}\n\nClick to choose another program."));
  program.icon = Icon::File(path.to_owned());
  program.action = Some(Action::ChooseProgram);
  rows.push(program);

  // Memory regions.
  if !regions.is_empty() {
    let mut group = Row::new("regions", None, "Memory Regions");
    group.icon = Icon::Named("layers");
    group.expanded = Some(true);
    rows.push(group);
    for region in regions {
      let usage = usage(elf, region);
      let percent = percent(usage.used, region.length);
      let id = format!("region:{}", region.name);
      let mut row = Row::new(&id, Some("regions"), &region.name);
      row.description = Some(format!(
        "{} of {} · {percent}%",
        size(usage.used),
        size(region.length)
      ));
      let last = region
        .origin
        .saturating_add(region.length)
        .saturating_sub(1);
      let mut tooltip = format!(
        "{}{} – {}\nUsed: {} ({} bytes)\nFree: {}",
        if region.attributes.is_empty() {
          String::new()
        } else {
          format!("({}) ", region.attributes)
        },
        address(region.origin, elf.bits),
        address(last, elf.bits),
        size(usage.used),
        usage.used,
        size(region.length.saturating_sub(usage.used)),
      );
      if usage.used > region.length {
        tooltip.push_str(&format!(
          "\nOverflowed by {}",
          size(usage.used - region.length)
        ));
      }
      row.tooltip = Some(tooltip);
      row.color = if usage.used > region.length || percent >= 100 {
        Some("full")
      } else if percent >= HIGH_PERCENT {
        Some("high")
      } else {
        None
      };
      row.badge = row.color.map(|_| "!".to_owned());
      row.action = Some(Action::Region(region.name.clone()));
      row.expanded = Some(false);
      rows.push(row);
      for (i, (section, at, bytes, load)) in usage.parts.iter().enumerate() {
        let label = if *load {
          format!("{section} (load image)")
        } else {
          section.clone()
        };
        let mut part = Row::new(format!("{id}:{i}"), Some(&id), label);
        part.description = Some(format!("{} · {}", address(*at, elf.bits), size(*bytes)));
        part.tooltip = Some(if *load {
          format!("{section}'s initial contents, loaded here and copied to where it runs.")
        } else {
          format!("{section} runs here.")
        });
        part.action = Some(Action::Section(section.clone()));
        rows.push(part);
      }
    }
  }

  // Segments.
  let loads: Vec<_> = elf
    .segments
    .iter()
    .filter(|segment| segment.kind == PT_LOAD)
    .collect();
  if !loads.is_empty() {
    let mut group = Row::new("segments", None, "Segments");
    group.icon = Icon::Named("list-flat");
    group.expanded = Some(regions.is_empty());
    rows.push(group);
    for (i, segment) in loads.iter().enumerate() {
      let id = format!("segment:{i}");
      let mut flags = String::new();
      flags.push(if segment.flags & 4 != 0 { 'R' } else { '-' });
      flags.push(if segment.flags & 2 != 0 { 'W' } else { '-' });
      flags.push(if segment.flags & 1 != 0 { 'X' } else { '-' });
      let mut row = Row::new(
        &id,
        Some("segments"),
        format!("LOAD {}", address(segment.vaddr, elf.bits)),
      );
      row.description = Some(format!("{flags} · {}", size(segment.memsz)));
      let mut tooltip = format!(
        "Runs at {}, loaded at {}\nIn the file: {} at offset 0x{:x}\nIn memory: {}",
        address(segment.vaddr, elf.bits),
        address(segment.paddr, elf.bits),
        size(segment.filesz),
        segment.offset,
        size(segment.memsz),
      );
      if segment.memsz > segment.filesz {
        tooltip.push_str(&format!(
          " ({} zero-filled)",
          size(segment.memsz - segment.filesz)
        ));
      }
      row.tooltip = Some(tooltip);
      let inside = elf.sections_in(segment);
      if !inside.is_empty() {
        row.expanded = Some(false);
      }
      rows.push(row);
      for section in inside {
        let mut part = Row::new(format!("{id}:{}", section.index), Some(&id), &section.name);
        part.description = Some(size(section.size));
        part.action = Some(Action::Section(section.name.clone()));
        rows.push(part);
      }
    }
  }

  // Sections, with their largest symbols.
  let mut group = Row::new("sections", None, "Sections");
  group.icon = Icon::Named("list-tree");
  group.expanded = Some(true);
  rows.push(group);
  let total: u64 = elf.allocated().map(|section| section.size).sum();
  for section in elf.allocated() {
    let id = format!("section:{}", section.index);
    let mut row = Row::new(&id, Some("sections"), &section.name);
    row.description = Some(format!(
      "{} · {}",
      address(section.addr, elf.bits),
      size(section.size)
    ));
    let load = elf.load_address(section);
    let mut tooltip = format!(
      "{} · {}\nRuns at {}",
      flags(section),
      if section.kind == SHT_NOBITS {
        "zero-filled, not in the file"
      } else {
        "in the file"
      },
      address(section.addr, elf.bits),
    );
    if load != section.addr {
      tooltip.push_str(&format!(", loaded at {}", address(load, elf.bits)));
    }
    tooltip.push_str(&format!(
      "\n{} ({} bytes, {}% of the program)",
      size(section.size),
      section.size,
      percent(section.size, total)
    ));
    row.tooltip = Some(tooltip);
    row.action = Some(Action::Section(section.name.clone()));
    let mut symbols: Vec<_> = elf
      .symbols
      .iter()
      .filter(|symbol| symbol.section == section.index)
      .collect();
    symbols.sort_by(|a, b| b.size.cmp(&a.size).then(a.value.cmp(&b.value)));
    if !symbols.is_empty() {
      row.expanded = Some(false);
    }
    let count = symbols.len();
    rows.push(row);
    for (i, symbol) in symbols.iter().take(MAX_SYMBOLS_SHOWN).enumerate() {
      let mut part = Row::new(format!("{id}:{i}"), Some(&id), &symbol.name);
      part.description = Some(format!(
        "{} · {}",
        size(symbol.size),
        address(symbol.value, elf.bits)
      ));
      part.tooltip = Some(format!(
        "{}\n{} at {} ({} bytes)",
        symbol.name,
        if symbol.kind == 2 { "Code" } else { "Data" },
        address(symbol.value, elf.bits),
        symbol.size
      ));
      part.icon = Icon::Named(if symbol.kind == 2 { "code" } else { "file" });
      part.action = Some(Action::Symbol(symbol.name.clone()));
      rows.push(part);
    }
    if count > MAX_SYMBOLS_SHOWN {
      let mut more = Row::new(
        format!("{id}:more"),
        Some(&id),
        format!("{} more", count - MAX_SYMBOLS_SHOWN),
      );
      more.description = Some("smaller".into());
      rows.push(more);
    }
  }
  rows
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::elf::tests::{build, reader};
  use crate::elf::{SHF_ALLOC, parse};

  fn program() -> Elf {
    let file = build(
      &[
        (
          ".text",
          1,
          SHF_ALLOC | SHF_EXECINSTR,
          0x0800_0000,
          0x3000,
          0x0800_0000,
        ),
        (
          ".data",
          1,
          SHF_ALLOC | SHF_WRITE,
          0x2000_0000,
          0x400,
          0x0800_3000,
        ),
        (
          ".bss",
          SHT_NOBITS,
          SHF_ALLOC | SHF_WRITE,
          0x2000_0400,
          0x1c00,
          0x2000_0400,
        ),
      ],
      &[
        ("main", 0x0800_0100, 0x80, 1),
        ("big", 0x0800_0200, 0x800, 1),
        ("table", 0x2000_0000, 0x100, 2),
      ],
    );
    parse(&mut reader(&file)).expect("parses")
  }

  fn regions() -> Vec<RegionSpan> {
    vec![
      RegionSpan {
        name: "FLASH".into(),
        origin: 0x0800_0000,
        length: 16 * 1024,
        attributes: "rx".into(),
      },
      RegionSpan {
        name: "RAM".into(),
        origin: 0x2000_0000,
        length: 8 * 1024,
        attributes: "rwx".into(),
      },
    ]
  }

  #[test]
  fn regions_count_what_runs_and_what_loads_there() {
    let elf = program();
    let flash = usage(&elf, &regions()[0]);
    assert_eq!(flash.used, 0x3000 + 0x400, ".text and .data's load image");
    assert_eq!(
      flash.parts,
      [
        (".text".into(), 0x0800_0000, 0x3000, false),
        (".data".into(), 0x0800_3000, 0x400, true)
      ]
    );
    let ram = usage(&elf, &regions()[1]);
    assert_eq!(ram.used, 0x2000, "full");
  }

  #[test]
  fn rows_show_regions_segments_sections_and_symbols() {
    let elf = program();
    let rows = rows(&elf, "D:\\fw\\build\\fw.elf", "build/fw.elf", &regions());
    let find = |id: &str| rows.iter().find(|row| row.id == id).expect(id);
    assert_eq!(find("program").label, "fw.elf");
    assert_eq!(
      find("program").description.as_deref(),
      Some("ELF64 · ARM · entry 0x08000000")
    );
    let flash = find("region:FLASH");
    assert_eq!(
      flash.description.as_deref(),
      Some("13.0 KiB of 16.0 KiB · 81%")
    );
    assert_eq!(flash.color, None);
    let ram = find("region:RAM");
    assert_eq!(ram.color, Some("full"));
    assert_eq!(ram.badge.as_deref(), Some("!"));
    assert_eq!(find("region:FLASH:1").label, ".data (load image)");
    assert_eq!(find("segment:0").label, "LOAD 0x08000000");
    assert_eq!(
      find("segment:0").description.as_deref(),
      Some("R-X · 12.0 KiB")
    );
    // Largest first.
    assert_eq!(find("section:1:0").label, "big");
    assert_eq!(find("section:1:1").label, "main");
    assert_eq!(
      find("section:1:0").action,
      Some(Action::Symbol("big".into()))
    );
    // Parents come before their children.
    for (i, row) in rows.iter().enumerate() {
      if let Some(parent) = &row.parent {
        assert!(rows[..i].iter().any(|r| &r.id == parent), "{}", row.id);
      }
    }
  }

  #[test]
  fn sizes_read_well() {
    assert_eq!(size(12), "12 B");
    assert_eq!(size(1536), "1.5 KiB");
    assert_eq!(size(3 * 1024 * 1024), "3.0 MiB");
    assert_eq!(address(0x1000, 32), "0x00001000");
    assert_eq!(address(0xffff_ffff_8000_0000, 64), "0xffffffff80000000");
  }
}
