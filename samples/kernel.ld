OUTPUT_FORMAT("elf64-x86-64", "elf64-x86-64", "elf64-x86-64")
OUTPUT_ARCH(i386:x86-64)
ENTRY(_start)
KERNEL_VMA = 0xffffffff80000000;

PHDRS
{
  text PT_LOAD FLAGS(5);
  rodata PT_LOAD FLAGS(4);
  data PT_LOAD FLAGS(6);
  dynamic PT_DYNAMIC FLAGS(6);
}

SECTIONS
{
  . = KERNEL_VMA + 1M;
  .text ALIGN(4K) : AT(ADDR(.text) - KERNEL_VMA) SUBALIGN(16)
  {
    *(EXCLUDE_FILE(*boot.o) .text .text.*)
    *(SORT_BY_ALIGNMENT(SORT_BY_NAME(.text.hot.*)))
    BYTE(0x90) SHORT(1) LONG(2) QUAD(3)
    FILL(0xCC)
    ASCIZ "flare"
  } :text = 0x90909090

  .rodata : ALIGN(4K) ONLY_IF_RO { *(.rodata .rodata.*) } :rodata
  .data (NOLOAD) : { *(.data) CONSTRUCTORS } :data
  .tdata (TYPE = SHT_PROGBITS) : { *(.tdata) }
  OVERLAY 0x1000 : NOCROSSREFS AT (0x4000)
  {
    .ov1 { ov1.o(.text) }
    .ov2 { ov2.o(.text) } :text
  }
  INCLUDE extra.ld
  . = DEFINED(__stack) ? __stack : . + 0x4000;
  __end = .;
  /DISCARD/ : { *(.comment) *(.note.*) }
}
INSERT AFTER .text;
REGION_ALIAS("REGION_TEXT", ROM);
EXTERN(main isr_table)
NOCROSSREFS(.text .data)
INPUT(crt0.o -lc AS_NEEDED(libm.so.6))
GROUP(libc.a libgcc.a)
SEARCH_DIR(/usr/lib); STARTUP(crt0.o)
FORCE_COMMON_ALLOCATION
VERSION {
  VERS_1.1 {
    global:
      foo1;
    local:
      old*;
  };
  VERS_1.2 {
    foo2;
  } VERS_1.1;
}
