# Progresso

- [x] Estrutura Cargo na raiz, sem dependências de terceiros no crate.
- [x] Núcleo SM83 com opcodes base e CB, interrupções, HALT, STOP, temporizador e serial executados por T-state.
- [x] Barramento de 16 bits, OAM DMA e cartuchos ROM ONLY, MBC1, MBC2, MBC3 e MBC5.
- [x] Vídeo DMG com quatro tons de cinza, camadas BG, Window e OBJ, modos LCD e interrupções STAT. APU com quatro canais e PCM estéreo.
- [x] CLI com protocolos Blargg e Mooneye, limite de execução, captura e comparação de framebuffer.
- [x] Frontend WebAssembly compilado e validado em Chromium com ROM pública, teclado, ponteiro, pausa e Web Audio.
- [x] 22 testes Rust e 117 ROMs públicas aprovadas. `halt_bug` e `dmg-acid2` coincidiram com referências independentes.
- [x] Release nativo, release WebAssembly, formatação e Clippy estrito verificados.
- [x] Extrator autocontido verificado em diretório limpo. Os 24 arquivos de texto ficaram idênticos byte por byte e a cópia passou nos testes e nos dois builds.
