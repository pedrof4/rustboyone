# Relatório de testes

Validação em 23 de novembro de 2026, Linux x86-64, Rust e Cargo 1.100.0, Python 3.12 e Chromium 156.0.8078.79.

## Aquisição

- Blargg foi baixado do repositório `retrio/gb-test-roms`, commit `c240dd7d700e5c0b00a7bbba52b53e4ee67b5f15`. [Repositório e revisão](https://github.com/retrio/gb-test-roms/tree/c240dd7d700e5c0b00a7bbba52b53e4ee67b5f15)
- Mooneye foi baixado como `mts-20260714-0944-31510e1.tar.xz`, SHA-256 `6d4fdda2f1d8d2f5f51b0ff3f6f3cc2fbae047aa395a39c82bda3a0e7cbd2641`. O código-fonte correspondente foi usado para investigar diferenças de temporização. [Distribuição oficial](https://gekkio.fi/files/mooneye-test-suite/mts-20260714-0944-31510e1/) · [Código-fonte](https://github.com/Gekkio/mooneye-test-suite/tree/31510e12eea6286d36eea060a6adde755e1067aa)
- `dmg-acid2.gb` e a imagem de referência DMG foram baixados da versão 1.0. A ROM tem SHA-256 `464e14b7d42e7feea0b7ede42be7071dc88913f75b9ffa444299424b63d1dff1`. A imagem de origem tem SHA-256 `ca966d50895c7efef05838590d148c2cbfd7fba57dab986f25b35b4da71abb57`. A conversão para PGM preservou as 23.040 amostras de cinza. [Release](https://github.com/mattcurrie/dmg-acid2/releases/tag/v1.0) · [Imagem DMG](https://github.com/mattcurrie/dmg-acid2/blob/v1.0/img/reference-dmg.png)
- A referência PGM de `halt_bug.gb` foi gerada com o SameBoy em modo DMG-B após 240 quadros, usando um boot sintético que inicializa SP, BGP, LCDC e entra em `0100`. A tela final dizia `Passed`. O resultado do DMG Emu após 120 VBlanks coincidiu byte por byte. [Revisão do SameBoy](https://github.com/LIJI32/SameBoy/tree/ea667ad79aa28409f034b1fd6da7304e1844750d)
- As ROMs mínimas dos testes de integração e o cartucho usado para verificar entrada e áudio no navegador foram montados sinteticamente. Os testes Rust geram suas fixtures em execução. A matriz abaixo usa ROMs públicas originais.

## Build e análise estática

| Comando | Resultado |
| --- | --- |
| `cargo test` | PASS · 2 unitários e 20 de integração |
| `cargo build --release` | PASS |
| `cargo clippy --all-targets -- -D warnings` | PASS |
| `cargo clippy --target wasm32-unknown-unknown --lib -- -D warnings` | PASS |
| `cargo fmt --check` | PASS |
| `cargo build --release --lib --target wasm32-unknown-unknown` | PASS |
| `python scripts/test_roms.py --cache work/verify-cache` | PASS · download a partir de cache vazio e execução da matriz |
| `python scripts/test_roms.py --offline` | PASS · execução com cache local |

## Testes Rust

O orçamento em T-states aparece quando o teste usa uma única sequência temporizada ou quando a fronteira é parte da asserção. As verificações diretas de componentes não têm uma execução de ROM única.

| Teste | T-states ou observação | Resultado | Diagnóstico |
| --- | --- | --- | --- |
| `daa_adjusts_all_packed_decimal_additions` | 10.000 operações diretas | PASS | DAA e flags em somas decimais |
| `daa_adjusts_all_packed_decimal_subtractions` | 10.000 operações diretas | PASS | DAA e flags em subtrações decimais |
| `decode_all_base_and_cb_opcodes` | Uma instrução por instância | PASS | 256 opcodes base e 256 CB, incluindo ciclos CB e opcodes inválidos |
| `arithmetic_flags_and_signed_stack_offset` | 88 | PASS | Z/N/H/C e deslocamento de SP |
| `cb_memory_bit_test_and_bit_writes` | 84 | PASS | RLC, BIT, SET e RES em `(HL)` |
| `conditional_control_and_stack_order` | 60 | PASS | CALL e RET condicionais, endereço na pilha |
| `delayed_ei_interrupt_priority_and_halt_bug` | 20 para a entrada da interrupção | PASS | Atraso de EI, prioridade e supressão de incremento do PC |
| `stop_freezes_clock_until_joypad_edge` | 2.048 em STOP | PASS | DIV e PPU congelados, borda de Joypad em IF |
| `timer_frequencies_falling_edges_and_reload_writes` | Períodos 1.024/16/64/256 e atraso 4 | PASS | Bordas em TAC/DIV, recarga, cancelamento e escrita TMA/TIMA |
| `serial_internal_and_external_transfer` | Oito pulsos por transferência | PASS | Saída `41 c5`, entrada serial externa `b3` |
| `memory_mirroring_and_dma_progress` | 160 bytes em M-cycles | PASS | Espelho WRAM, FEA0, fases de DMA e HRAM disponível |
| `cartridge_validation_and_rom_only_ram` | Acessos diretos | PASS | Cabeçalho, tamanho, RAM e save |
| `mbc1_rom_modes_and_ram_banks` | Acessos diretos | PASS | ROM 0/32/33 e alternância de RAM |
| `mbc2_address_bit_ram_nibble_and_mirror` | Acessos diretos | PASS | A8, RAM de quatro bits e espelhamento |
| `mbc3_rtc_latch_halt_and_day_carry` | Avanços de 4.194.304 e 8.388.608 | PASS | Latch, halt, dia 511 e carry |
| `mbc5_ninth_rom_bit_and_ram_bank` | Acessos diretos | PASS | Nono bit da ROM e bancos de RAM |
| `ppu_modes_stat_and_line_153` | 456 por linha normal | PASS | Modos 0–3, STAT, VBlank e LY na linha 153 |
| `background_window_and_sprite_priority_limit` | 908 após LCDC | PASS | BG, Window, precedência, prioridade OBJ e limite 10 |
| `sprite_8x16_flip_and_second_palette` | 908 após LCDC | PASS | Tile par, flips e OBP1 |
| `apu_four_channels_and_pcm_output` | 8.192 de geração | PASS | CH1–4 ativos, PCM estéreo e DAC |
| `cli_stops_when_serial_matches_and_times_out_when_missing` | 3.636 / 10.000 | PASS | Serial `51`, parada positiva e falha por limite |
| `cli_captures_and_compares_pgm_images` | 65.660 por execução | PASS | PGM idêntico e divergência deliberada de um pixel |

## Frontend WebAssembly

| Caso | T-states | Resultado | Diagnóstico |
| --- | ---: | --- | --- |
| Arquivo malformado sintético | 0 | PASS | Erro de cabeçalho apresentado na interface |
| `dmg-acid2.gb` | 769236 | PASS | Canvas com 23.040 amostras idênticas à referência oficial |
| ROM sintética de entrada e áudio | 978120 | PASS | Tecla Z e botão A alteraram a paleta na ROM ao pressionar e soltar. Pausa, retomada e Web Audio estéreo a 32.768 Hz verificados. |

## Triagem de modelos

A primeira enumeração incluiu três testes específicos do Super Game Boy. Eles foram retirados da seleção DMG-B na rodada de aceite junto com as variantes destinadas a DMG-0, MGB e SGB2.

| ROM da triagem Mooneye | T-states | Resultado no DMG-B | Serial | Diagnóstico |
| --- | ---: | --- | --- | --- |
| `boot_div-S.gb` | 741372 | FAIL | `52` | Registradores `42`, valor/ fase de DIV do SGB |
| `boot_div2-S.gb` | 741388 | FAIL | `52` | Registradores `42`, valor/ fase de DIV do SGB |
| `boot_hwio-S.gb` | 727548 | FAIL | `52` | Registradores `42`, estado de I/O do SGB |

## ROMs públicas selecionadas

A matriz inclui os testes de aceitação compatíveis com DMG-B e os testes de mapeadores do Mooneye, os casos Blargg de CPU, memória, instruções e HALT, além do `dmg-acid2`. O Blargg usa o texto serial ou o código de status na RAM do cartucho. No Mooneye, usam-se os registradores B/C/D/E/H/L após o marcador final `LD B,B`. O campo Serial mostra os bytes que efetivamente chegaram pelo link DMG. Em `halt_bug` e `dmg-acid2`, o hash do PGM verifica todos os pixels.

**117/117 PASS.**

| ROM | T-states | Resultado | Serial | Diagnóstico |
| --- | ---: | --- | --- | --- |
| `Blargg/cpu_instrs/cpu_instrs.gb` | 224436788 | PASS | `"cpu_instrs\n\n01:ok  02:ok  03:ok  04:ok  05:ok  06:ok  07:ok  08:ok  09:ok  10:ok  11:ok  \n\nPassed all tests\n"` | Blargg, mensagem serial |
| `Blargg/instr_timing/instr_timing.gb` | 2703932 | PASS | `"instr_timing\n\n\nPassed\n"` | Blargg, mensagem serial |
| `Blargg/mem_timing/mem_timing.gb` | 6391348 | PASS | `"mem_timing\n\n01:ok  02:ok  03:ok  \n\nPassed all tests\n"` | Blargg, mensagem serial |
| `Blargg/mem_timing-2/mem_timing.gb` | 11622628 | PASS | `""` | Blargg, status do cartucho · status=00 texto="mem_timing\n\n01:ok  02:ok  03:ok  \n\nPassed\n" |
| `Blargg/cpu_instrs/individual/01-special.gb` | 9796660 | PASS | `"01-special\n\n\nPassed\n"` | Blargg, mensagem serial |
| `Blargg/cpu_instrs/individual/02-interrupts.gb` | 1791028 | PASS | `"02-interrupts\n\n\nPassed\n"` | Blargg, mensagem serial |
| `Blargg/cpu_instrs/individual/03-op sp,hl.gb` | 9866812 | PASS | `"03-op sp,hl\n\n\nPassed\n"` | Blargg, mensagem serial |
| `Blargg/cpu_instrs/individual/04-op r,imm.gb` | 11481656 | PASS | `"04-op r,imm\n\n\nPassed\n"` | Blargg, mensagem serial |
| `Blargg/cpu_instrs/individual/05-op rp.gb` | 15766076 | PASS | `"05-op rp\n\n\nPassed\n"` | Blargg, mensagem serial |
| `Blargg/cpu_instrs/individual/06-ld r,r.gb` | 2423356 | PASS | `"06-ld r,r\n\n\nPassed\n"` | Blargg, mensagem serial |
| `Blargg/cpu_instrs/individual/07-jr,jp,call,ret,rst.gb` | 2985020 | PASS | `"07-jr,jp,call,ret,rst\n\n\nPassed\n"` | Blargg, mensagem serial |
| `Blargg/cpu_instrs/individual/08-misc instrs.gb` | 2283064 | PASS | `"08-misc instrs\n\n\nPassed\n"` | Blargg, mensagem serial |
| `Blargg/cpu_instrs/individual/09-op r,r.gb` | 38307900 | PASS | `"09-op r,r\n\n\nPassed\n"` | Blargg, mensagem serial |
| `Blargg/cpu_instrs/individual/10-bit ops.gb` | 58181684 | PASS | `"10-bit ops\n\n\nPassed\n"` | Blargg, mensagem serial |
| `Blargg/cpu_instrs/individual/11-op a,(hl).gb` | 73771068 | PASS | `"11-op a,(hl)\n\n\nPassed\n"` | Blargg, mensagem serial |
| `Blargg/mem_timing/individual/01-read_timing.gb` | 1931828 | PASS | `"01-read_timing\n\n\nPassed\n"` | Blargg, mensagem serial |
| `Blargg/mem_timing/individual/02-write_timing.gb` | 1861692 | PASS | `"02-write_timing\n\n\nPassed\n"` | Blargg, mensagem serial |
| `Blargg/mem_timing/individual/03-modify_timing.gb` | 2072116 | PASS | `"03-modify_timing\n\n\nPassed\n"` | Blargg, mensagem serial |
| `Blargg/mem_timing-2/rom_singles/01-read_timing.gb` | 2001908 | PASS | `""` | Blargg, status do cartucho · status=00 texto="01-read_timing\n\n\nPassed\n" |
| `Blargg/mem_timing-2/rom_singles/02-write_timing.gb` | 1931712 | PASS | `""` | Blargg, status do cartucho · status=00 texto="02-write_timing\n\n\nPassed\n" |
| `Blargg/mem_timing-2/rom_singles/03-modify_timing.gb` | 2142364 | PASS | `""` | Blargg, status do cartucho · status=00 texto="03-modify_timing\n\n\nPassed\n" |
| `Blargg/halt_bug.gb` | 8602712 | PASS | `—` | PGM SHA-256 igual: d82792e6823d29ac643fca19fcf17edd156af90974746c1c993e03266d78628b |
| `Mooneye/acceptance/add_sp_e_timing.gb` | 950928 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/bits/mem_oam.gb` | 742952 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/bits/reg_f.gb` | 738876 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/bits/unused_hwio-GS.gb` | 724512 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/boot_div-dmgABCmgb.gb` | 739840 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/boot_hwio-dmgABCmgb.gb` | 724532 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/boot_regs-dmgABC.gb` | 741520 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/call_cc_timing.gb` | 935444 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/call_cc_timing2.gb` | 1020980 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/call_timing.gb` | 935444 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/call_timing2.gb` | 1020984 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/di_timing-GS.gb` | 1076912 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/div_timing.gb` | 738944 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/ei_sequence.gb` | 738864 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/ei_timing.gb` | 738836 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/halt_ime0_ei.gb` | 725028 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/halt_ime0_nointr_timing.gb` | 1019760 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/halt_ime1_timing.gb` | 738332 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/halt_ime1_timing2-GS.gb` | 1301152 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/if_ie_registers.gb` | 739376 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/instr/daa.gb` | 1567276 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/interrupts/ie_push.gb` | 726248 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/intr_timing.gb` | 738636 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/jp_cc_timing.gb` | 935460 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/jp_timing.gb` | 935460 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/ld_hl_sp_e_timing.gb` | 950908 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/oam_dma/basic.gb` | 746012 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/oam_dma/reg_read.gb` | 731148 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/oam_dma/sources-GS.gb` | 2550836 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/oam_dma_restart.gb` | 949296 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/oam_dma_start.gb` | 1034784 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/oam_dma_timing.gb` | 949300 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/pop_timing.gb` | 739484 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/ppu/hblank_ly_scx_timing-GS.gb` | 3322900 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/ppu/intr_1_2_timing-GS.gb` | 949292 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/ppu/intr_2_0_timing.gb` | 879140 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/ppu/intr_2_mode0_timing.gb` | 879140 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/ppu/intr_2_mode0_timing_sprites.gb` | 16733752 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/ppu/intr_2_mode3_timing.gb` | 879140 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/ppu/intr_2_oam_ok_timing.gb` | 879140 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/ppu/lcdon_timing-GS.gb` | 2123288 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/ppu/lcdon_write_timing-GS.gb` | 3914776 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/ppu/stat_irq_blocking.gb` | 795164 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/ppu/stat_lyc_onoff.gb` | 988176 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/ppu/vblank_stat_intr-GS.gb` | 1300952 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/push_timing.gb` | 949776 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/rapid_di_ei.gb` | 739368 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/ret_cc_timing.gb` | 1005608 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/ret_timing.gb` | 1005608 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/reti_intr_timing.gb` | 738872 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/reti_timing.gb` | 1005608 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/rst_timing.gb` | 950912 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/serial/boot_sclk_align-dmgABCmgb.gb` | 740384 | PASS | `00 00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/timer/div_write.gb` | 3604684 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/timer/rapid_toggle.gb` | 738860 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/timer/tim00.gb` | 738480 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/timer/tim00_div_trigger.gb` | 738456 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/timer/tim01.gb` | 738548 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/timer/tim01_div_trigger.gb` | 738604 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/timer/tim10.gb` | 738528 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/timer/tim10_div_trigger.gb` | 738544 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/timer/tim11.gb` | 738772 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/timer/tim11_div_trigger.gb` | 738720 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/timer/tima_reload.gb` | 740136 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/timer/tima_write_reloading.gb` | 739544 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/acceptance/timer/tma_write_reloading.gb` | 740288 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/emulator-only/mbc1/bits_bank1.gb` | 12523172 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/emulator-only/mbc1/bits_bank2.gb` | 12311572 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/emulator-only/mbc1/bits_mode.gb` | 12523132 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/emulator-only/mbc1/bits_ramg.gb` | 24460288 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/emulator-only/mbc1/multicart_rom_8Mb.gb` | 935440 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/emulator-only/mbc1/ram_256kb.gb` | 3884600 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/emulator-only/mbc1/ram_64kb.gb` | 3884576 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/emulator-only/mbc1/rom_16Mb.gb` | 935440 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/emulator-only/mbc1/rom_1Mb.gb` | 935440 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/emulator-only/mbc1/rom_2Mb.gb` | 935440 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/emulator-only/mbc1/rom_4Mb.gb` | 935440 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/emulator-only/mbc1/rom_512kb.gb` | 935440 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/emulator-only/mbc1/rom_8Mb.gb` | 935440 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/emulator-only/mbc2/bits_ramg.gb` | 24951860 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/emulator-only/mbc2/bits_romb.gb` | 13084164 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/emulator-only/mbc2/bits_unused.gb` | 12241448 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/emulator-only/mbc2/ram.gb` | 1988636 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/emulator-only/mbc2/rom_1Mb.gb` | 724492 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/emulator-only/mbc2/rom_2Mb.gb` | 724492 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/emulator-only/mbc2/rom_512kb.gb` | 724492 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/emulator-only/mbc5/rom_16Mb.gb` | 935472 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/emulator-only/mbc5/rom_1Mb.gb` | 935472 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/emulator-only/mbc5/rom_2Mb.gb` | 935472 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/emulator-only/mbc5/rom_32Mb.gb` | 935472 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/emulator-only/mbc5/rom_4Mb.gb` | 935472 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/emulator-only/mbc5/rom_512kb.gb` | 935472 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/emulator-only/mbc5/rom_64Mb.gb` | 935472 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `Mooneye/emulator-only/mbc5/rom_8Mb.gb` | 935472 | PASS | `00` | Mooneye, registradores Fibonacci no LD B,B |
| `dmg-acid2/dmg-acid2.gb` | 8953160 | PASS | `—` | PGM SHA-256 igual: b2fd578e3564e82e82b4e796d9e4bf735c36fa26a52d55c78132eb225aad72e3 |

## Reconstrução

`python extract_dmg_emu.py` recriou 24 arquivos de texto em um diretório temporário. A comparação byte por byte incluiu o próprio extrator. Uma segunda execução recusou o destino existente e preservou seus arquivos. A cópia reconstruída passou em `cargo test`, `cargo build --release` e no build de biblioteca para `wasm32-unknown-unknown`.
