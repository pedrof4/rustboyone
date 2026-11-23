# DMG Emu

Emulador do Game Boy DMG em Rust, com runner de linha de comando e frontend WebAssembly. O crate depende apenas da biblioteca padrão.

## Jogar no navegador

A cópia no workspace inclui `web/dmg_emu.wasm` compilado. Execute na raiz do projeto e abra o endereço local exibido pelo servidor.

```sh
python scripts/serve_web.py --no-build
```

Para compilar o WebAssembly, inclusive após reconstruir o projeto com o extrator, use Rust estável com rustup e Python 3.12 ou posterior. O processo funciona no Windows, Linux e macOS.

```sh
rustup target add wasm32-unknown-unknown
python scripts/serve_web.py
```

Abra uma ROM pelo botão **Abrir ROM**. As setas controlam o direcional, **Z** é A, **X** é B, **Shift** é Select e **Enter** é Start. Os botões na página também aceitam toque. O botão **Som** ativa a saída de áudio. A RAM de cartuchos com bateria é armazenada localmente no navegador quando o navegador permite armazenamento.

## Runner headless

```sh
cargo build --release
cargo run --release -- jogo.gb --frames 120 --frame captura.pgm
cargo run --release -- teste.gb --cycles 300000000 --protocol blargg
cargo run --release -- teste.gb --cycles 30000000 --protocol mooneye
```

Use `--serial-contains TEXTO` para parar em uma mensagem personalizada. Use `--expect-frame ARQUIVO` para comparar um PGM no mesmo formato produzido pela CLI. Use `--battery jogo.sav` para ler e gravar a RAM de um cartucho com bateria. O orçamento de ciclos encerra a execução na fronteira de instrução seguinte. Códigos de saída 0, 1 e 2 representam sucesso, falha de verificação e erro de entrada ou I/O.

## Validação

```sh
cargo test
cargo clippy --all-targets -- -D warnings
python scripts/test_roms.py
```

O script de ROMs usa Git e Python 3.12 ou posterior. Na primeira execução, baixa revisões fixadas das suítes para `work/`. Nas execuções seguintes, `--offline` usa o cache. A matriz de resultados fica em `work/rom-results.md`. Consulte `TEST_REPORT.md` para a execução de entrega.

## Reconstrução

```sh
python extract_dmg_emu.py
```

O script cria `dmg-emu/` no diretório atual, com os fontes, testes, documentos e scripts. Se houver um diretório com esse nome, o script preserva seus arquivos. O WebAssembly pode ser recompilado a partir dos fontes.

Veja `ARCHITECTURE.md` para o modelo de execução e o escopo de precisão.
