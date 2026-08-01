# ADR 0005: Checkpoint de empacotamento

- Status: aceita (parcial — só a perna Linux foi verificada de verdade)
- Data: 2026-07-31

## Contexto

CLAUDE.md §3 estima ~10 MB de bundle para a Rota A (Rust puro) contra ~50 MB da Rota B
(sidecar Python). `PLAN.md` recomenda um checkpoint de empacotamento logo após o item 4 do
roadmap, gerando o instalador no CI e instalando de fato no SO alvo — assinatura, notarização
e antivírus são o tipo de problema que só aparece tarde e custa dias se não for verificado
cedo.

Este ambiente de execução é Linux (Fedora 44, container). Só a perna Linux pôde ser
verificada de ponta a ponta; Windows e macOS ficam registrados como pendência explícita, não
como "verificado por suposição".

## Evidência (Linux)

`pnpm tauri build` (com `--bundle`, não `--no-bundle`) gerou:

| Artefato | Tamanho | Verificado |
|---|---|---|
| `.rpm` (`queryboard-0.1.0-1.x86_64.rpm`) | 10,4 MB comprimido / 34,3 MB instalado | Instalado de verdade via `sudo rpm -ivh`, binário `/usr/bin/queryboard` executado sob Xvfb, **duas janelas reais abertas** (confirmado via `xdotool search`), depois desinstalado via `rpm -e` |
| `.deb` (`queryboard_0.1.0_amd64.deb`) | 10,4 MB comprimido | Gerado com sucesso; não instalado (este ambiente é Fedora, não Debian/Ubuntu — instalar um `.deb` aqui exigiria `alien` ou um container à parte, fora do escopo deste checkpoint) |
| AppImage (`queryboard_0.1.0_amd64.AppImage`) | não gerado | **Falhou**: `failed to run linuxdeploy`. `linuxdeploy` é ele mesmo um AppImage que precisa montar via FUSE; `/dev/fuse` existe neste container, mas o mount falhou mesmo assim — comportamento comum em ambientes sandboxed/rootless que bloqueiam `CAP_SYS_ADMIN` mesmo com o device node presente. Não é um problema do `queryboard`, é uma limitação deste ambiente de execução especificamente |

O tamanho do `.rpm`/`.deb` (10,4 MB) bate quase exatamente com a estimativa de ~10 MB do
CLAUDE.md §3 para a Rota A — confirma que a escolha de driver Rust puro (Postgres, por ora)
entrega o bundle pequeno que justificava considerar Tauri em primeiro lugar.

## Problema encontrado e corrigido

O `identifier` original (`com.queryboard.app`) terminava em `.app`, que o próprio Tauri
avisa ser incompatível com a extensão de bundle do macOS (`.app` é a extensão dos bundles de
aplicativo macOS; um identifier terminando assim quebra a geração do `.app`/`.dmg`). Trocado
para `dev.queryboard.desktop` — o aviso desapareceu nos builds seguintes.

## Problema encontrado e registrado, não corrigido

O campo `license` do bundle (`tauri.conf.json` → `bundle.license`) está vazio — o `.rpm`
gerado tem `License: ` em branco. Não decidido aqui de propósito: a escolha de licença é do
usuário, não algo para um agente decidir silenciosamente. **Questão em aberto**, registrada
para quando o projeto sair do estágio "nome de trabalho" (CLAUDE.md §1).

## Pendências explícitas (não verificadas nesta execução)

- **Windows**: `.msi`/`.exe` (NSIS) não gerados nem testados — exigiria um runner Windows.
  Assinatura de código Windows (Authenticode) não configurada; sem ela, o instalador dispara
  o aviso do SmartScreen. Fica para quando houver um runner Windows disponível (o job `build`
  do CI já cobre `windows-latest` na matriz, mas ninguém rodou o binário resultante de
  verdade num Windows real ainda).
- **macOS**: `.app`/`.dmg` não gerados nem testados. Assinatura (`codesign`) e notarização
  (`notarytool`) não configuradas — sem elas, o Gatekeeper bloqueia a abertura do app com
  "desenvolvedor não identificado". Exige conta Apple Developer paga; decisão de negócio, não
  técnica, fica pendente do usuário.
- **`.deb` em ambiente Debian/Ubuntu real**: gerado mas não instalado de fato (este ambiente
  é Fedora). Formato é padrão o bastante para ter alta confiança, mas "gerado com sucesso"
  não é o mesmo padrão de evidência que "instalado e rodando" — registrar a diferença.
- **AppImage**: bloqueado pela limitação de FUSE deste ambiente especificamente. Revisitar
  num ambiente com suporte real a FUSE (a maioria das máquinas de desenvolvedor tem).

## Consequências

- O checkpoint confirma que a Rota A entrega o bundle pequeno prometido — pelo menos para
  Postgres. Quando o driver Oracle for decidido (ADR 0001), medir de novo: se a decisão for
  Rota B (sidecar Python), o bundle deve saltar para a faixa de ~50 MB estimada, e isso
  precisa de outro checkpoint dedicado antes do item 5 do roadmap (driver Oracle) ser dado
  como pronto.
- CI (`build` job, matriz `ubuntu-latest`/`windows-latest`/`macos-latest`) já roda
  `pnpm tauri build` nos três SOs a cada push — mas isso testa se o build **compila e
  empacota**, não se o instalador resultante **instala e roda** num SO real fora do runner.
  Continua sendo trabalho manual verificar isso em máquinas de verdade antes de qualquer
  release.
- `identifier` corrigido agora, antes de qualquer usuário ter instalado uma versão com o
  identifier antigo — trocar depois de haver instalações reais exigiria migração de dados
  (o identifier normalmente participa do caminho de `$APPDATA` em alguns SOs).
