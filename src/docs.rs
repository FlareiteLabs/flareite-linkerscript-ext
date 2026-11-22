//! What ld's commands and functions do, for hover and completion. After the
//! GNU ld manual (Linker Scripts), in short.

/// Where a word can be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
  /// At the top of a script.
  Top,
  /// In SECTIONS, between output sections.
  Sections,
  /// In an output section's `{ }`.
  Body,
  /// After an output section's `:`.
  Attribute,
  /// In an expression.
  Function,
  /// A section's type: `(NOLOAD)`.
  Type,
}

pub struct Doc {
  pub name: &'static str,
  pub usage: &'static str,
  pub text: &'static str,
  pub places: &'static [Place],
}

use Place::*;

pub const DOCS: &[Doc] = &[
  // ----- Commands -----
  Doc {
    name: "ENTRY",
    usage: "ENTRY(symbol)",
    text: "The program's entry point: the first instruction run. The `-e` option wins over it.",
    places: &[Top, Sections],
  },
  Doc {
    name: "INCLUDE",
    usage: "INCLUDE filename",
    text: "Reads another linker script here, found in the current folder or a `-L` folder.",
    places: &[Top, Sections, Body],
  },
  Doc {
    name: "INPUT",
    usage: "INPUT(file file ...)",
    text: "Links these files, as if named on the command line. `-lfile` searches for `libfile.a`.",
    places: &[Top],
  },
  Doc {
    name: "GROUP",
    usage: "GROUP(file file ...)",
    text: "Like INPUT, but the files are archives searched again and again until no new undefined references are made.",
    places: &[Top],
  },
  Doc {
    name: "AS_NEEDED",
    usage: "AS_NEEDED(file file ...)",
    text: "Inside INPUT or GROUP: shared libraries linked only if something uses them.",
    places: &[],
  },
  Doc {
    name: "OUTPUT",
    usage: "OUTPUT(filename)",
    text: "The output file's name, like `-o filename` (which wins).",
    places: &[Top],
  },
  Doc {
    name: "SEARCH_DIR",
    usage: "SEARCH_DIR(path)",
    text: "Adds a folder to search for libraries, like `-L path`.",
    places: &[Top],
  },
  Doc {
    name: "STARTUP",
    usage: "STARTUP(filename)",
    text: "Links this file first, as if it came first on the command line.",
    places: &[Top],
  },
  Doc {
    name: "OUTPUT_FORMAT",
    usage: "OUTPUT_FORMAT(default, big, little)",
    text: "The BFD format of the output, such as `elf64-x86-64` or `elf32-littlearm`; with three, the one for `-EB` or `-EL`.",
    places: &[Top],
  },
  Doc {
    name: "OUTPUT_ARCH",
    usage: "OUTPUT_ARCH(architecture)",
    text: "The machine the output is for, such as `i386:x86-64`, `arm`, or `riscv`.",
    places: &[Top],
  },
  Doc {
    name: "TARGET",
    usage: "TARGET(bfdname)",
    text: "The BFD format of the input files that follow, like `-b bfdname`.",
    places: &[Top],
  },
  Doc {
    name: "EXTERN",
    usage: "EXTERN(symbol symbol ...)",
    text: "Makes these symbols undefined, so they're linked in from archives, like `-u symbol`.",
    places: &[Top],
  },
  Doc {
    name: "NOCROSSREFS",
    usage: "NOCROSSREFS(section section ...)",
    text: "An error if code in one of these output sections refers to another of them.",
    places: &[Top],
  },
  Doc {
    name: "NOCROSSREFS_TO",
    usage: "NOCROSSREFS_TO(to from from ...)",
    text: "An error if one of the other sections refers to the first.",
    places: &[Top],
  },
  Doc {
    name: "REGION_ALIAS",
    usage: "REGION_ALIAS(alias, region)",
    text: "Another name for a memory region, so one script can place sections in different regions on different boards.",
    places: &[Top],
  },
  Doc {
    name: "LD_FEATURE",
    usage: "LD_FEATURE(\"SANE_EXPR\")",
    text: "Turns on a feature of ld; `SANE_EXPR` treats numbers outside SECTIONS as numbers, not addresses.",
    places: &[Top],
  },
  Doc {
    name: "INSERT",
    usage: "INSERT AFTER section | INSERT BEFORE section",
    text: "Adds this script's SECTIONS to the default script's, after or before an output section, instead of replacing it.",
    places: &[Top],
  },
  Doc {
    name: "FORCE_COMMON_ALLOCATION",
    usage: "FORCE_COMMON_ALLOCATION",
    text: "Gives common symbols space even in a relocatable link (`-r`).",
    places: &[Top],
  },
  Doc {
    name: "INHIBIT_COMMON_ALLOCATION",
    usage: "INHIBIT_COMMON_ALLOCATION",
    text: "Gives common symbols no space, even in a final link.",
    places: &[Top],
  },
  Doc {
    name: "FORCE_GROUP_ALLOCATION",
    usage: "FORCE_GROUP_ALLOCATION",
    text: "Places section group members like other sections and removes the groups, even in a relocatable link.",
    places: &[Top],
  },
  Doc {
    name: "PROVIDE",
    usage: "PROVIDE(symbol = expression)",
    text: "Defines a symbol only if the program uses it and doesn't define it, as `end` and `etext` are.",
    places: &[Top, Sections, Body],
  },
  Doc {
    name: "PROVIDE_HIDDEN",
    usage: "PROVIDE_HIDDEN(symbol = expression)",
    text: "Like PROVIDE, but the symbol is hidden: not exported from the output.",
    places: &[Top, Sections, Body],
  },
  Doc {
    name: "HIDDEN",
    usage: "HIDDEN(symbol = expression)",
    text: "Defines a hidden symbol: not exported from the output.",
    places: &[Top, Sections, Body],
  },
  Doc {
    name: "ASSERT",
    usage: "ASSERT(expression, \"message\")",
    text: "Stops the link with the message if the expression is zero, such as `ASSERT(_end <= ORIGIN(RAM) + LENGTH(RAM), \"out of RAM\")`.",
    places: &[Top, Sections, Body, Function],
  },
  Doc {
    name: "SECTIONS",
    usage: "SECTIONS { ... }",
    text: "How input sections go into output sections, and where those go in memory. Without it, ld uses its default layout.",
    places: &[Top],
  },
  Doc {
    name: "MEMORY",
    usage: "MEMORY { name (attributes) : ORIGIN = address, LENGTH = size ... }",
    text: "The regions of memory sections can be placed in, with `> region`. Attributes say which input sections go in a region by default: `r` read-only, `w` writable, `x` executable, `a` allocatable, `i`/`l` initialized, `!` for the opposite.",
    places: &[Top],
  },
  Doc {
    name: "PHDRS",
    usage: "PHDRS { name type [FILEHDR] [PHDRS] [AT(address)] [FLAGS(flags)]; ... }",
    text: "The ELF program headers (segments), in place of ld's own. Sections name theirs with `:name` after their `}`.",
    places: &[Top],
  },
  Doc {
    name: "VERSION",
    usage: "VERSION { node { global: symbols; local: *; }; ... }",
    text: "Symbol versions for a shared library, as in a version script.",
    places: &[Top],
  },
  Doc {
    name: "OVERLAY",
    usage: "OVERLAY [start] : [NOCROSSREFS] [AT(lma)] { section { ... } ... } [> region]",
    text: "Sections that share one run address, loaded at different addresses, to be copied in as they're needed.",
    places: &[Sections],
  },
  // ----- In an output section -----
  Doc {
    name: "KEEP",
    usage: "KEEP(*(.section))",
    text: "Keeps these input sections when unused sections are removed (`--gc-sections`), as vector tables and `.init` must be.",
    places: &[Body],
  },
  Doc {
    name: "SORT",
    usage: "SORT(pattern)",
    text: "Another name for SORT_BY_NAME.",
    places: &[Body],
  },
  Doc {
    name: "SORT_BY_NAME",
    usage: "SORT_BY_NAME(pattern)",
    text: "Places the matching files or sections in order of name.",
    places: &[Body],
  },
  Doc {
    name: "SORT_BY_ALIGNMENT",
    usage: "SORT_BY_ALIGNMENT(pattern)",
    text: "Places the matching sections in order of alignment, largest first, which can save padding.",
    places: &[Body],
  },
  Doc {
    name: "SORT_BY_INIT_PRIORITY",
    usage: "SORT_BY_INIT_PRIORITY(pattern)",
    text: "Places the matching sections in order of the priority in their names (`.init_array.65535`), for constructors.",
    places: &[Body],
  },
  Doc {
    name: "SORT_NONE",
    usage: "SORT_NONE(pattern)",
    text: "Keeps the matching sections in the order they come, whatever `--sort-section` says.",
    places: &[Body],
  },
  Doc {
    name: "REVERSE",
    usage: "REVERSE(pattern)",
    text: "Places the matching sections in the reverse of their order.",
    places: &[Body],
  },
  Doc {
    name: "EXCLUDE_FILE",
    usage: "EXCLUDE_FILE(file ...) pattern",
    text: "Leaves out the input sections of these files: `*(EXCLUDE_FILE(*crtend.o) .ctors)`.",
    places: &[Body],
  },
  Doc {
    name: "INPUT_SECTION_FLAGS",
    usage: "INPUT_SECTION_FLAGS(SHF_WRITE & !SHF_EXECINSTR) *(.data)",
    text: "Takes only the input sections whose ELF flags match.",
    places: &[Body],
  },
  Doc {
    name: "BYTE",
    usage: "BYTE(expression)",
    text: "Puts one byte of data here.",
    places: &[Body],
  },
  Doc {
    name: "SHORT",
    usage: "SHORT(expression)",
    text: "Puts two bytes of data here, in the output's byte order.",
    places: &[Body],
  },
  Doc {
    name: "LONG",
    usage: "LONG(expression)",
    text: "Puts four bytes of data here, in the output's byte order.",
    places: &[Body],
  },
  Doc {
    name: "QUAD",
    usage: "QUAD(expression)",
    text: "Puts eight bytes of data here.",
    places: &[Body],
  },
  Doc {
    name: "SQUAD",
    usage: "SQUAD(expression)",
    text: "Puts eight bytes of data here, sign-extended on 32-bit hosts.",
    places: &[Body],
  },
  Doc {
    name: "FILL",
    usage: "FILL(expression)",
    text: "The bytes that fill gaps in the rest of this section, such as `FILL(0x90909090)`.",
    places: &[Body],
  },
  Doc {
    name: "ASCIZ",
    usage: "ASCIZ \"string\"",
    text: "Puts a string here, with a zero byte after it.",
    places: &[Body],
  },
  Doc {
    name: "CREATE_OBJECT_SYMBOLS",
    usage: "CREATE_OBJECT_SYMBOLS",
    text: "Makes a symbol for each input file, named after it.",
    places: &[Body],
  },
  Doc {
    name: "CONSTRUCTORS",
    usage: "CONSTRUCTORS",
    text: "Places C++ constructor tables here, for formats without arbitrary sections (not ELF, which uses `.ctors`).",
    places: &[Body],
  },
  Doc {
    name: "LINKER_VERSION",
    usage: "LINKER_VERSION",
    text: "Puts ld's version here, as a string.",
    places: &[Body],
  },
  Doc {
    name: "COMMON",
    usage: "*(COMMON)",
    text: "Common symbols: uninitialized globals the compiler didn't put in `.bss`.",
    places: &[],
  },
  // ----- After an output section's colon -----
  Doc {
    name: "AT",
    usage: "AT(address)  ·  AT> region",
    text: "Where the section is loaded, when that isn't where it runs (its LMA): initialized data loaded in flash and copied to RAM at startup.",
    places: &[Attribute],
  },
  Doc {
    name: "ALIGN_WITH_INPUT",
    usage: "ALIGN_WITH_INPUT",
    text: "Keeps the difference between the section's run and load addresses as the run address is aligned.",
    places: &[Attribute],
  },
  Doc {
    name: "SUBALIGN",
    usage: "SUBALIGN(alignment)",
    text: "Aligns each input section in it to this, whatever its own alignment.",
    places: &[Attribute],
  },
  Doc {
    name: "ONLY_IF_RO",
    usage: "ONLY_IF_RO",
    text: "Creates the section only if all its input sections are read-only.",
    places: &[Attribute],
  },
  Doc {
    name: "ONLY_IF_RW",
    usage: "ONLY_IF_RW",
    text: "Creates the section only if all its input sections are writable.",
    places: &[Attribute],
  },
  // ----- Types -----
  Doc {
    name: "NOLOAD",
    usage: "(NOLOAD)",
    text: "The section isn't loaded: it takes no space in the file, like `.bss`, and memory there keeps what it had.",
    places: &[Type],
  },
  Doc {
    name: "READONLY",
    usage: "(READONLY)",
    text: "The section is read-only, whatever its input sections are.",
    places: &[Type],
  },
  Doc {
    name: "INFO",
    usage: "(INFO)",
    text: "The section isn't allocated in memory, like debug information.",
    places: &[Type],
  },
  Doc {
    name: "COPY",
    usage: "(COPY)",
    text: "Like INFO: not allocated in memory.",
    places: &[Type],
  },
  Doc {
    name: "DSECT",
    usage: "(DSECT)",
    text: "Like INFO: not allocated in memory.",
    places: &[Type],
  },
  Doc {
    name: "TYPE",
    usage: "(TYPE = SHT_PROGBITS)",
    text: "The section's ELF type.",
    places: &[Type],
  },
  // ----- Functions -----
  Doc {
    name: "ABSOLUTE",
    usage: "ABSOLUTE(expression)",
    text: "The expression's value as an absolute address, not relative to its section.",
    places: &[Function],
  },
  Doc {
    name: "ADDR",
    usage: "ADDR(section)",
    text: "The address the output section runs at (its VMA).",
    places: &[Function],
  },
  Doc {
    name: "ALIGN",
    usage: "ALIGN(alignment)  ·  ALIGN(expression, alignment)",
    text: "The location counter (or the expression) rounded up to a multiple of the alignment, a power of two. After an output section's colon, it aligns the section.",
    places: &[Function, Attribute],
  },
  Doc {
    name: "ALIGNOF",
    usage: "ALIGNOF(section)",
    text: "The output section's alignment, once it's placed.",
    places: &[Function],
  },
  Doc {
    name: "BLOCK",
    usage: "BLOCK(alignment)",
    text: "Another name for ALIGN, from old scripts.",
    places: &[Function],
  },
  Doc {
    name: "CONSTANT",
    usage: "CONSTANT(MAXPAGESIZE)  ·  CONSTANT(COMMONPAGESIZE)",
    text: "A constant of the target: its largest or usual page size.",
    places: &[Function],
  },
  Doc {
    name: "DATA_SEGMENT_ALIGN",
    usage: "DATA_SEGMENT_ALIGN(maxpagesize, commonpagesize)",
    text: "Where the data segment starts, chosen to save memory pages; ld's default scripts use it.",
    places: &[Function],
  },
  Doc {
    name: "DATA_SEGMENT_END",
    usage: "DATA_SEGMENT_END(expression)",
    text: "Marks the end of the data segment, for DATA_SEGMENT_ALIGN.",
    places: &[Function],
  },
  Doc {
    name: "DATA_SEGMENT_RELRO_END",
    usage: "DATA_SEGMENT_RELRO_END(offset, expression)",
    text: "Marks the end of the part made read-only after relocation (`-z relro`).",
    places: &[Function],
  },
  Doc {
    name: "DEFINED",
    usage: "DEFINED(symbol)",
    text: "1 if the symbol is defined so far, else 0: `stack = DEFINED(stack) ? stack : 0x8000;`.",
    places: &[Function],
  },
  Doc {
    name: "LENGTH",
    usage: "LENGTH(region)",
    text: "The memory region's length.",
    places: &[Function],
  },
  Doc {
    name: "LOADADDR",
    usage: "LOADADDR(section)",
    text: "The address the output section is loaded at (its LMA): where startup code copies initialized data from.",
    places: &[Function],
  },
  Doc {
    name: "LOG2CEIL",
    usage: "LOG2CEIL(expression)",
    text: "The base-2 logarithm of the expression, rounded up.",
    places: &[Function],
  },
  Doc {
    name: "MAX",
    usage: "MAX(expression, expression)",
    text: "The larger of the two.",
    places: &[Function],
  },
  Doc {
    name: "MIN",
    usage: "MIN(expression, expression)",
    text: "The smaller of the two.",
    places: &[Function],
  },
  Doc {
    name: "NEXT",
    usage: "NEXT(expression)",
    text: "The next unallocated address that's a multiple of the expression.",
    places: &[Function],
  },
  Doc {
    name: "ORIGIN",
    usage: "ORIGIN(region)",
    text: "The memory region's start address.",
    places: &[Function],
  },
  Doc {
    name: "SEGMENT_START",
    usage: "SEGMENT_START(\"segment\", default)",
    text: "Where a segment starts if `-T segment=address` says, else the default.",
    places: &[Function],
  },
  Doc {
    name: "SIZEOF",
    usage: "SIZEOF(section)",
    text: "The output section's size in bytes.",
    places: &[Function],
  },
  Doc {
    name: "SIZEOF_HEADERS",
    usage: "SIZEOF_HEADERS",
    text: "The size of the output's headers, often where the first section starts.",
    places: &[Function],
  },
  Doc {
    name: "MAXPAGESIZE",
    usage: "CONSTANT(MAXPAGESIZE)",
    text: "The target's largest page size.",
    places: &[],
  },
  Doc {
    name: "COMMONPAGESIZE",
    usage: "CONSTANT(COMMONPAGESIZE)",
    text: "The target's usual page size.",
    places: &[],
  },
  // ----- MEMORY -----
  Doc {
    name: "ORIGIN ",
    usage: "ORIGIN = address  (or org, o)",
    text: "Where the memory region starts.",
    places: &[],
  },
  Doc {
    name: "LENGTH ",
    usage: "LENGTH = size  (or len, l)",
    text: "The memory region's size, such as `64K` or `1M`.",
    places: &[],
  },
  // ----- Program headers -----
  Doc {
    name: "FILEHDR",
    usage: "FILEHDR",
    text: "The segment includes the ELF file header.",
    places: &[],
  },
  Doc {
    name: "FLAGS",
    usage: "FLAGS(flags)",
    text: "The segment's `p_flags`: 4 read, 2 write, 1 execute (5 for code, 6 for data).",
    places: &[],
  },
];

/// The words ld uses that the parser doesn't call commands, for hover in
/// MEMORY: `ORIGIN =` and `LENGTH =`.
pub fn memory_word(word: &str) -> Option<&'static Doc> {
  let name = match word {
    "ORIGIN" | "org" | "o" => "ORIGIN ",
    "LENGTH" | "len" | "l" => "LENGTH ",
    _ => return None,
  };
  DOCS.iter().find(|doc| doc.name == name)
}

pub fn find(name: &str) -> Option<&'static Doc> {
  DOCS.iter().find(|doc| doc.name == name)
}

/// The program header types.
pub const PHDR_TYPES: &[(&str, &str)] = &[
  ("PT_NULL", "Unused."),
  ("PT_LOAD", "Loaded into memory."),
  ("PT_DYNAMIC", "Dynamic linking information."),
  ("PT_INTERP", "The program interpreter's path."),
  ("PT_NOTE", "Notes."),
  ("PT_SHLIB", "Reserved."),
  ("PT_PHDR", "The program headers themselves."),
  ("PT_TLS", "Thread-local storage."),
  ("PT_GNU_EH_FRAME", "The exception handling frame table."),
  ("PT_GNU_STACK", "Whether the stack is executable."),
  ("PT_GNU_RELRO", "Made read-only after relocation."),
];

/// Input sections compilers make, offered inside `*( )`.
pub const INPUT_SECTIONS: &[(&str, &str)] = &[
  (".text", "Code"),
  (
    ".text.*",
    "Code, one section per function (-ffunction-sections)",
  ),
  (".rodata", "Read-only data"),
  (
    ".rodata.*",
    "Read-only data, one section per object (-fdata-sections)",
  ),
  (".data", "Initialized data"),
  (".data.*", "Initialized data, one section per object"),
  (".bss", "Zero-initialized data"),
  (".bss.*", "Zero-initialized data, one section per object"),
  ("COMMON", "Common symbols"),
  (".init", "Code run before main"),
  (".fini", "Code run after main"),
  (".init_array", "Constructors"),
  (".init_array.*", "Constructors with a priority"),
  (".fini_array", "Destructors"),
  (".fini_array.*", "Destructors with a priority"),
  (".preinit_array", "Run before constructors"),
  (".ctors", "Old-style constructors"),
  (".dtors", "Old-style destructors"),
  (".eh_frame", "Exception handling frames"),
  (".eh_frame_hdr", "Exception handling frame table"),
  (".gcc_except_table", "Exception handling tables"),
  (".tdata", "Thread-local initialized data"),
  (".tbss", "Thread-local zero-initialized data"),
  (".comment", "Compiler version strings"),
  (".note.*", "Notes"),
  (".ARM.exidx*", "ARM exception index"),
  (".ARM.extab*", "ARM exception tables"),
  (".isr_vector", "Interrupt vector table (Cortex-M)"),
  (".got", "Global offset table"),
  (".got.plt", "Global offset table for the PLT"),
  (".plt", "Procedure linkage table"),
  (".rela.*", "Relocations"),
  (".dynamic", "Dynamic linking information"),
];
