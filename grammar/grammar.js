/**
 * @file GNU ld linker scripts, for Flareite's Linker Script extension.
 * @license MIT
 *
 * Based on Amaan Qureshi's tree-sitter-linkerscript (MIT, see LICENSE),
 * extended to the commands GNU ld's own grammar (ld/ldgram.y) accepts:
 * the file and format commands, output section attributes, input section
 * sorting and exclusions, data commands, OVERLAY, PHDRS, and VERSION.
 */

/* eslint-disable arrow-parens */
/* eslint-disable camelcase */
/// <reference types="tree-sitter-cli/dsl" />
// @ts-check

const PREC = {
  CONDITIONAL: -1,
  LOGICAL_OR: 1,
  LOGICAL_AND: 2,
  INCLUSIVE_OR: 3,
  BITWISE_AND: 5,
  EQUAL: 6,
  RELATIONAL: 7,
  SHIFT: 9,
  ADD: 10,
  MULTIPLY: 11,
  UNARY: 14,
  CALL: 15,
};

// Characters that can't be part of a file name or wildcard pattern.
const NOT_NAME = '\\s(){};,"=';

module.exports = grammar({
  name: 'linkerscript',

  extras: $ => [$.comment, /\s/],

  supertypes: $ => [$.expression],

  word: $ => $.symbol,

  rules: {
    linkerscript: $ => repeat($._command),

    _command: $ => choice(
      $.assignment,
      $.provide_command,
      $.assert_command,
      $.entry_command,
      $.include_command,
      $.input_command,
      $.file_command,
      $.format_command,
      $.arch_command,
      $.target_command,
      $.extern_command,
      $.nocrossrefs_command,
      $.region_alias_command,
      $.feature_command,
      $.insert_command,
      $.flag_command,
      $.sections_command,
      $.memory_command,
      $.phdrs_command,
      $.version_command,
      ';',
    ),

    // ENTRY(_start)
    entry_command: $ => seq('ENTRY', '(', field('name', $.symbol), ')'),

    // INCLUDE common.ld
    include_command: $ => seq('INCLUDE', field('path', $.filename)),

    // INPUT(crt0.o -lc AS_NEEDED(libm.so)), GROUP(...)
    input_command: $ => seq(
      field('command', choice('INPUT', 'GROUP')),
      '(',
      repeat(choice($.filename, $.as_needed, ',')),
      ')',
    ),

    as_needed: $ => seq('AS_NEEDED', '(', repeat(choice($.filename, ',')), ')'),

    // OUTPUT(a.out), SEARCH_DIR(/usr/lib), STARTUP(crt0.o)
    file_command: $ => seq(
      field('command', choice('OUTPUT', 'SEARCH_DIR', 'STARTUP')),
      '(',
      field('path', $.filename),
      ')',
    ),

    // OUTPUT_FORMAT("elf32-littlearm", "elf32-bigarm", "elf32-littlearm")
    format_command: $ => seq(
      'OUTPUT_FORMAT',
      '(',
      commaSep1(field('format', $._format_name)),
      ')',
    ),

    // OUTPUT_ARCH(i386:x86-64)
    arch_command: $ => seq('OUTPUT_ARCH', '(', field('architecture', $._format_name), ')'),

    // TARGET(binary)
    target_command: $ => seq('TARGET', '(', field('format', $._format_name), ')'),

    _format_name: $ => choice(
      $.string,
      alias($.symbol, $.format_name),
      alias($._pattern, $.format_name),
    ),

    // EXTERN(main reset_handler)
    extern_command: $ => seq('EXTERN', '(', repeat(choice($.symbol, ',')), ')'),

    // NOCROSSREFS(.text .data)
    nocrossrefs_command: $ => seq(
      field('command', choice('NOCROSSREFS', 'NOCROSSREFS_TO')),
      '(',
      repeat(choice(field('section', $.symbol), ',')),
      ')',
    ),

    // REGION_ALIAS("REGION_TEXT", ROM)
    region_alias_command: $ => seq(
      'REGION_ALIAS',
      '(',
      field('alias', choice($.string, $.symbol)),
      ',',
      field('region', $.symbol),
      ')',
    ),

    // LD_FEATURE("SANE_EXPR")
    feature_command: $ => seq('LD_FEATURE', '(', $.string, ')'),

    // INSERT AFTER .text
    insert_command: $ => seq('INSERT', choice('AFTER', 'BEFORE'), field('section', $.symbol)),

    flag_command: _ => choice(
      'FORCE_COMMON_ALLOCATION',
      'INHIBIT_COMMON_ALLOCATION',
      'FORCE_GROUP_ALLOCATION',
    ),

    // symbol = expression;
    assignment: $ => seq(
      field('left', choice($.symbol, $.string, $.location_counter)),
      field('operator', choice('=', '+=', '-=', '*=', '/=', '<<=', '>>=', '&=', '|=', '^=')),
      field('right', $.expression),
      choice(';', ','),
    ),

    // PROVIDE(end = .); PROVIDE_HIDDEN(...); HIDDEN(...)
    provide_command: $ => seq(
      field('command', choice('PROVIDE', 'PROVIDE_HIDDEN', 'HIDDEN')),
      '(',
      field('name', choice($.symbol, $.string)),
      '=',
      field('value', $.expression),
      ')',
    ),

    // ASSERT(. <= 0x8000, "too big")
    assert_command: $ => seq(
      'ASSERT',
      '(',
      field('condition', $.expression),
      ',',
      field('message', choice($.string, $.symbol)),
      ')',
    ),

    sections_command: $ => seq(
      'SECTIONS',
      '{',
      repeat($._sections_item),
      '}',
    ),

    _sections_item: $ => choice(
      $.output_section,
      $.overlay,
      $.assignment,
      $.provide_command,
      $.assert_command,
      $.entry_command,
      $.include_command,
      ';',
    ),

    // .text 0x1000 (NOLOAD) : AT(0x2000) ALIGN(16) { ... } > RAM AT> ROM :text = 0x90
    output_section: $ => seq(
      field('name', choice($.symbol, $.string, $.discard)),
      optional(field('address', $.expression)),
      optional(field('type', $.section_type)),
      ':',
      repeat(choice(
        $.load_address,
        $.section_align,
        $.subalign,
        $.section_constraint,
      )),
      field('body', $.section_body),
      optional($.region),
      optional($.load_region),
      repeat($.phdr),
      optional($.fill),
      optional(','),
    ),

    discard: _ => '/DISCARD/',

    section_type: $ => seq(
      '(',
      choice(
        'NOLOAD',
        'DSECT',
        'COPY',
        'INFO',
        'OVERLAY',
        'READONLY',
        seq('TYPE', '=', $.expression),
        seq('READONLY', '(', 'TYPE', '=', $.expression, ')'),
      ),
      ')',
    ),

    load_address: $ => seq('AT', '(', field('address', $.expression), ')'),

    section_align: $ => choice(
      seq('ALIGN', '(', field('alignment', $.expression), ')'),
      'ALIGN_WITH_INPUT',
    ),

    subalign: $ => seq('SUBALIGN', '(', field('alignment', $.expression), ')'),

    section_constraint: _ => choice('ONLY_IF_RO', 'ONLY_IF_RW', 'SPECIAL'),

    section_body: $ => seq('{', repeat($._section_item), '}'),

    // > RAM
    region: $ => seq('>', field('name', $.symbol)),

    // AT> ROM
    load_region: $ => seq('AT', '>', field('name', $.symbol)),

    // :text
    phdr: $ => seq(':', field('name', $.symbol)),

    // = 0x90909090
    fill: $ => seq('=', field('value', $.expression)),

    _section_item: $ => choice(
      $.input_section,
      $.keep_command,
      $.assignment,
      $.provide_command,
      $.assert_command,
      $.include_command,
      $.data_command,
      $.fill_command,
      $.asciz_command,
      $.section_keyword,
      ';',
      ',',
    ),

    // *(.text .text.*), crt0.o(.text), *(COMMON), libc.a:*(.data)
    input_section: $ => prec.right(seq(
      optional($.section_flags),
      field('file', $._file_spec),
      optional(seq(
        '(',
        repeat(choice(field('section', $._section_spec), ',')),
        ')',
      )),
    )),

    // INPUT_SECTION_FLAGS(SHF_WRITE & !SHF_EXECINSTR)
    section_flags: $ => seq(
      'INPUT_SECTION_FLAGS',
      '(',
      repeat1(choice($.symbol, '&', '!')),
      ')',
    ),

    _file_spec: $ => choice(
      $.pattern,
      $.sort,
      $.exclude_file,
    ),

    _section_spec: $ => choice(
      $.pattern,
      $.sort,
      $.exclude_file,
    ),

    pattern: $ => choice(alias($.symbol, 'name'), $._pattern),

    // SORT_BY_NAME(.text.*)
    sort: $ => seq(
      field('command', choice(
        'SORT',
        'SORT_BY_NAME',
        'SORT_BY_ALIGNMENT',
        'SORT_BY_INIT_PRIORITY',
        'SORT_NONE',
        'REVERSE',
      )),
      '(',
      repeat1(choice($.pattern, $.sort, $.exclude_file)),
      ')',
    ),

    // EXCLUDE_FILE(*crtend.o *otherfile.o)
    exclude_file: $ => seq(
      'EXCLUDE_FILE',
      '(',
      repeat(choice($.pattern, ',')),
      ')',
    ),

    // KEEP(*(.init))
    keep_command: $ => seq('KEEP', '(', repeat1($.input_section), ')'),

    // LONG(0), BYTE(1)
    data_command: $ => seq(
      field('command', choice('BYTE', 'SHORT', 'LONG', 'QUAD', 'SQUAD')),
      '(',
      field('value', $.expression),
      ')',
    ),

    // FILL(0x90)
    fill_command: $ => seq('FILL', '(', field('value', $.expression), ')'),

    // ASCIZ "version 1"
    asciz_command: $ => seq('ASCIZ', $.string),

    section_keyword: _ => choice('CREATE_OBJECT_SYMBOLS', 'CONSTRUCTORS', 'LINKER_VERSION'),

    // OVERLAY 0x1000 : AT(0x4000) { .a { *(.a) } .b { *(.b) } } > RAM
    overlay: $ => seq(
      'OVERLAY',
      optional(field('start', $.expression)),
      ':',
      optional('NOCROSSREFS'),
      optional($.load_address),
      '{',
      repeat($.overlay_section),
      '}',
      optional($.region),
      repeat($.phdr),
      optional($.fill),
      optional(','),
    ),

    overlay_section: $ => seq(
      field('name', choice($.symbol, $.string)),
      field('body', $.section_body),
      repeat($.phdr),
      optional($.fill),
      optional(','),
    ),

    memory_command: $ => seq(
      'MEMORY',
      '{',
      repeat(choice($.memory_region, $.include_command, ';')),
      '}',
    ),

    // FLASH (rx) : ORIGIN = 0x08000000, LENGTH = 64K
    memory_region: $ => seq(
      field('name', $.symbol),
      optional(seq('(', field('attributes', $.attributes), ')')),
      ':',
      choice('ORIGIN', 'org', 'o'),
      '=',
      field('origin', $.expression),
      optional(','),
      choice('LENGTH', 'len', 'l'),
      '=',
      field('length', $.expression),
    ),

    attributes: _ => /[rwxailRWXAIL!]+/,

    phdrs_command: $ => seq(
      'PHDRS',
      '{',
      repeat($.program_header),
      '}',
    ),

    // text PT_LOAD FILEHDR PHDRS AT(0x1000) FLAGS(5);
    program_header: $ => seq(
      field('name', $.symbol),
      field('type', choice($.symbol, $.number)),
      repeat(choice(
        'FILEHDR',
        'PHDRS',
        seq('AT', '(', field('address', $.expression), ')'),
        seq('FLAGS', '(', field('flags', $.expression), ')'),
      )),
      ';',
    ),

    // VERSION { VERS_1 { global: foo; local: *; }; }
    version_command: $ => seq('VERSION', '{', repeat($.version_node), '}'),

    version_node: $ => seq(
      optional(field('name', $.symbol)),
      '{',
      repeat(choice(
        seq(choice('global', 'local'), ':'),
        seq('extern', $.string, '{', repeat(choice($._version_symbol, ';')), '}'),
        seq($._version_symbol, ';'),
      )),
      '}',
      repeat(field('parent', $.symbol)),
      ';',
    ),

    _version_symbol: $ => choice(
      alias($.symbol, $.pattern),
      alias($._version_pattern, $.pattern),
      $.string,
    ),

    // A wildcard or C++ name in a version script: `old*`, `ns::f`. Unlike
    // `_pattern`, it needs a wildcard or `::`, so `global:` isn't one.
    _version_pattern: _ => token(/[^\s{};":]*([*?\[\]]|::)[^\s{};"]*/),

    expression: $ => choice(
      $.conditional_expression,
      $.unary_expression,
      $.binary_expression,
      $.parenthesized_expression,
      $.call_expression,
      $.symbol,
      $.string,
      $.number,
      $.location_counter,
    ),

    location_counter: _ => '.',

    conditional_expression: $ => prec.right(PREC.CONDITIONAL, seq(
      field('condition', $.expression),
      '?',
      field('consequence', $.expression),
      ':',
      field('alternative', $.expression),
    )),

    unary_expression: $ => prec.left(PREC.UNARY, seq(
      field('operator', choice('!', '~', '-', '+')),
      field('argument', $.expression),
    )),

    binary_expression: $ => {
      const table = [
        ['+', PREC.ADD],
        ['-', PREC.ADD],
        ['*', PREC.MULTIPLY],
        ['/', PREC.MULTIPLY],
        ['%', PREC.MULTIPLY],
        ['||', PREC.LOGICAL_OR],
        ['&&', PREC.LOGICAL_AND],
        ['|', PREC.INCLUSIVE_OR],
        ['&', PREC.BITWISE_AND],
        ['==', PREC.EQUAL],
        ['!=', PREC.EQUAL],
        ['>', PREC.RELATIONAL],
        ['>=', PREC.RELATIONAL],
        ['<=', PREC.RELATIONAL],
        ['<', PREC.RELATIONAL],
        ['<<', PREC.SHIFT],
        ['>>', PREC.SHIFT],
      ];

      return choice(...table.map(([operator, precedence]) => prec.left(precedence, seq(
        field('left', $.expression),
        // @ts-ignore
        field('operator', operator),
        field('right', $.expression),
      ))));
    },

    parenthesized_expression: $ => seq('(', $.expression, ')'),

    // ALIGN(4K), SIZEOF(.text), CONSTANT(MAXPAGESIZE)
    call_expression: $ => prec(PREC.CALL, seq(
      field('function', $.symbol),
      field('arguments', $.argument_list),
    )),

    argument_list: $ => seq('(', commaSep($.expression), ')'),

    number: _ => token(choice(
      /0[xX][0-9a-fA-F]+[KkMm]?/,
      /\$[0-9a-fA-F]+/,
      /[0-9][0-9a-fA-F]*[hHxX]/,
      /[0-9]+[oOqQbBdD]?/,
      /[0-9]+[KkMm]/,
    )),

    symbol: _ => /[a-zA-Z_.][a-zA-Z0-9_.$-]*/,

    // A file name or wildcard that isn't also a symbol: it has a character
    // a symbol can't, or starts with one a symbol can't start with.
    _pattern: _ => token(choice(
      new RegExp(`[0-9-][a-zA-Z0-9_.$-]*([^${NOT_NAME}]*)`),
      new RegExp(`[a-zA-Z0-9_.$-]*[^${NOT_NAME}a-zA-Z0-9_.$-][^${NOT_NAME}]*`),
    )),

    filename: $ => choice($.string, alias($.symbol, 'name'), $._pattern),

    string: _ => token(seq('"', /[^"\n]*/, '"')),

    comment: _ => token(choice(
      seq('/*', /[^*]*\*+([^/*][^*]*\*+)*/, '/'),
      seq('#', /.*/),
    )),
  },
});

/**
 * One or more of `rule`, separated by commas.
 *
 * @param {Rule} rule
 * @return {SeqRule}
 */
function commaSep1(rule) {
  return seq(rule, repeat(seq(',', rule)));
}

/**
 * Any number of `rule`, separated by commas.
 *
 * @param {Rule} rule
 * @return {ChoiceRule}
 */
function commaSep(rule) {
  return optional(commaSep1(rule));
}
