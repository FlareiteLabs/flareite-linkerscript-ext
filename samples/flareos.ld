/* FlareOS kernel layout: identity-mapped, physical == virtual, at 1 MiB.
 * The bootloader copies each PT_LOAD segment to its p_paddr and zeroes BSS. */

ENTRY(_start)

SECTIONS
{
    . = 0x100000;

    .text : ALIGN(4K)
    {
        KEEP(*(.text.entry))
        *(.text .text.*)
    }

    .rodata : ALIGN(4K)
    {
        *(.rodata .rodata.*)
    }

    .data : ALIGN(4K)
    {
        *(.data .data.*)
    }

    .bss : ALIGN(4K)
    {
        *(.bss .bss.*)
        *(COMMON)
    }

    /DISCARD/ :
    {
        *(.eh_frame .eh_frame_hdr)
        *(.comment)
        *(.note .note.*)
    }
}
