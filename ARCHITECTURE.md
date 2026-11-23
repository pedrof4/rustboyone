# Arquitetura

O crate Rust usa a biblioteca padrão. A CLI e o navegador executam a mesma máquina `GameBoy`, composta por CPU e barramento.

| Módulo | Responsabilidade |
| --- | --- |
| `cpu` | Decodifica os grupos SM83 e CB. Cada acesso à memória ou ciclo interno consome um M-cycle. Trata prioridade das interrupções, EI adiado, HALT, HALT bug e STOP. |
| `bus` | Endereços de 16 bits, espelho WRAM, HRAM, I/O, IE e IF. Avança os periféricos quatro T-states a cada M-cycle e coordena DMA, joypad e serial. |
| `timer` | Acumula DIV interno de 16 bits. Aplica as bordas descendentes selecionadas pelo TAC e o atraso de recarga de TIMA. |
| `cartridge` | Valida tamanho e tipo de ROM, seleciona bancos e disponibiliza RAM externa. RTC do MBC3 com latch, halt, dias e carry. |
| `ppu` | Sequencia linhas, modos LCD, STAT e acesso de CPU a VRAM e OAM. Produz shades 0–3 em uma grade 160×144. |
| `apu` | Dois canais de pulso, onda programável e ruído, sequenciador acionado por DIV, mixer estéreo a 32.768 Hz. |
| `main` | Runner nativo com orçamento em T-states, protocolos de teste e imagem PGM P5. |
| `wasm` | Fronteira de memória WebAssembly para ROM, joypad, pixels, PCM e saves. |

A inicialização usa estado pós-boot DMG. O relógio base é 4.194.304 T-states/s. A linha ativa dura 456 T-states e o quadro contínuo dura 70.224. CPU, serial, LCD e DMA continuam durante HALT. STOP congela os periféricos internos até a entrada de joypad.

A PPU seleciona até dez sprites em ordem de OAM e dá precedência ao menor X, com desempate pelo primeiro índice. Durante o desenho, avalia a saída de cada pixel e inclui atrasos gerados pelo alinhamento de SCX, Window e OBJ. O deslocamento vertical da Window avança nas linhas em que ela desenhou.

O frontend WebAssembly usa Canvas, eventos de teclado e ponteiro, Web Audio e armazenamento local de RAM identificada pelo conteúdo da ROM. Um servidor Python da biblioteca padrão publica o frontend apenas em localhost.

## Escopo de precisão

A máquina usa o estado de saída do boot do DMG nas revisões A/B/C e inicia no endereço `0100`. O RTC do MBC3 avança por tempo emulado e mantém o latch, halt e carry enquanto o cartucho está carregado. A persistência cobre a RAM de bateria.

A temporização dos modos LCD, a composição de pixels e as bordas de STAT são verificadas nas ROMs do relatório. O modelo calcula a cor do pixel em saída a partir dos mapas e da posição do fetch. Transições de registradores dentro do mesmo fetch e a corrupção elétrica de OAM em modo 2 ficam fora desse modelo. O áudio cobre registros, comprimento, envelope, sweep, temporização de forma de onda e mixagem PCM, com aproximações na saída analógica e nos efeitos de acesso à RAM de onda.
