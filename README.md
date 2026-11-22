# Linker Script

GNU ld linker scripts for Flareite (`.ld` and `.lds` files), which LLVM's lld reads too.

- **Highlighting** from a Tree-sitter grammar (in `grammar/`, based on
  [amaanq/tree-sitter-linkerscript](https://github.com/amaanq/tree-sitter-linkerscript) and
  extended to the commands GNU ld accepts): commands, output sections, input sections, memory
  regions, program headers, ld's functions, and the location counter. SECTIONS, MEMORY, output
  sections, and comments fold.
- **Go to Symbol** (Ctrl+Shift+O) lists the output sections, memory regions, symbols, and program
  headers. **Go to Definition** (F12, or Ctrl+click) and **Find References** work within a script:
  from `> FLASH` to the MEMORY line that declares it, from `SIZEOF(.text)` to `.text`.
- **Problems as you type**: syntax errors, as GNU ld's grammar has them; memory regions, program
  headers, and output sections that aren't declared; regions declared twice; and INCLUDEs that
  can't be found. Scripts that INCLUDE each other are read together, so `> FLASH` in
  `sections.ld` finds the MEMORY in `memory.ld`. A misspelled command, function, or name has a fix
  (Ctrl+.): `ENTRYY` to `ENTRY`, `> FLAHS` to `> FLASH`.
- **The linker's problems when you save**: the linker links nothing with the script (the one
  given to the linker, if the saved one is INCLUDEd), which reads and evaluates all of it, and what
  it reports goes to Problems. It runs ld.lld, or else rust-lld (which rustup installs with Rust),
  with no object files and no output written: symbols the script uses but doesn't define are
  defined for it (they'd come from your objects), and what an empty program can't satisfy, such as
  its ASSERTs and the entry symbol, is left out. **Check Linker Script with the Linker** runs it
  without saving.
- **Completion** of what fits where you are: commands at the top, in SECTIONS, and in an output
  section; AT, ALIGN, and SUBALIGN after a section's colon; NOLOAD and the other types; your
  memory regions after `>` and in `ORIGIN()`, sections in `SIZEOF()` and `ADDR()`, program headers
  after `:`, and symbols; ld's functions in expressions; the sections compilers make inside
  `*( )`; and other scripts after INCLUDE.
- **Hover** says what ld's commands and functions do, where a memory region starts and ends and
  how large it is, where an output section runs and is loaded, and a symbol's definition (and its
  value, when the script gives one).
- **The Memory Layout view** (Linker, in the activity bar) shows the program your build links: how
  full each memory region is (marked at 90% and when full), with the sections in it and the load
  images of initialized data; its segments; and its sections with their largest symbols (Rust's
  names demangled). Click a region, section, or symbol to go to it in your script. Choose the
  program in the view, or set it per folder with the `program` setting; the view follows rebuilds.

## Settings

| Setting     | Default | What it does                                                                                                                       |
| ----------- | ------- | ---------------------------------------------------------------------------------------------------------------------------------- |
| `linker`    | `auto`  | What checks scripts when they're saved: `auto` (ld.lld, else rust-lld), `ld.lld`, `rust-lld`, `ld` (GNU ld), or `off`.              |
| `emulation` | (none)  | The linker's `-m`, such as `armelf` or `elf32lriscv`. By default it's from OUTPUT_FORMAT or OUTPUT_ARCH, else `elf_x86_64` for lld. |
| `arguments` | (none)  | More arguments for the linker, such as `-L ldscripts`.                                                                             |
| `program`   | (none)  | The program the Memory Layout view shows, relative to the folder: `target/x86_64-unknown-none/debug/kernel`.                       |

The linker runs only in folders you trust. To use one that isn't on the PATH, set where it is in
`settings.toml`:

```toml
[extensions.programs]
"flareite.linker-script.lld" = "C:/Program Files/LLVM/bin/ld.lld.exe"
# GNU ld for your target, with linker = "ld":
"flareite.linker-script.ld" = "C:/arm-gnu-toolchain/bin/arm-none-eabi-ld.exe"
```

GNU ld can link only for the targets it was built for, so choose the one for your target (such
as `arm-none-eabi-ld`); lld links for every target it knows.

## Building

```sh
cargo build --release
```

The grammar is built from `grammar/grammar.js` with the Tree-sitter CLI, into
`grammars/tree-sitter-linkerscript.wasm`:

```sh
cd grammar
npx tree-sitter-cli@0.27.1 generate
npx tree-sitter-cli@0.27.1 build --wasm -o ../grammars/tree-sitter-linkerscript.wasm .
```

`samples/` has scripts the tests parse: GNU ld's own default scripts, a Cortex-M one, and a
kernel's.

## License

MIT or Apache-2.0. The grammar is MIT, after Amaan Qureshi's (`grammar/LICENSE`).
