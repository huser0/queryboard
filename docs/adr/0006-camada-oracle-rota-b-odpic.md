# ADR 0006: Camada de acesso ao Oracle — Rota B revisada (crate `oracle`/ODPI-C, não sidecar Python)

- Status: aceita — substitui a recomendação da ADR 0001
- Data: 2026-08-04

## Contexto

A ADR 0001 (2026-07-31) já tinha decidido descartar a Rota A (`oracledb`, crate Rust puro)
por causa de um bloqueio real: a dependência `asupersync` exige Rust nightly
(`#![feature(try_trait_v2)]` no feature-set `default`, não contornável do lado do
consumidor). Isso permanece verdade e não muda aqui.

Onde esta ADR diverge da 0001: a recomendação de Rota B registrada lá era o sidecar
`python-oracledb`, com o crate `oracle` (ODPI-C) explicitamente relegado a "terceira via
registrada, não escolhida" por causa do Instant Client. A 0001 também deixou como
**follow-up explícito e pendente**: rodar os checks K1–K9 contra um Oracle real (não
apenas os checks P1–P3, que não dependem de banco).

Esta sessão executou esse follow-up pendente — e, ao executá-lo, teve acesso a informação
que a 0001 não tinha: uma validação completa, de ponta a ponta, do crate `oracle`
(ODPI-C) contra um Oracle Database Free real (`gvenzl/oracle-free:23-slim`, local, via
`docker compose --profile oracle`), incluindo cancelamento, sessão somente-leitura e
sanitização de erro — nenhum desses tinha evidência real na 0001.

## Decisão

**Rota B passa a ser o crate `oracle` (ODPI-C, síncrono, `kubo/rust-oracle`) — não mais o
sidecar `python-oracledb`.** O crate roda dentro do mesmo binário Rust, sem processo
externo, sem protocolo IPC, sem empacotamento de interpretador Python.

## Evidência

Spike completo contra Oracle Database Free real (mesmo container que agora é o serviço
`oracle` do `docker-compose.yml`, atrás de `--profile oracle`):

- **Conexão via EasyConnect** (`host:porta/serviço`): funciona.
- **`NUMBER` de alta precisão**: `NUMBER(38,10)` decodifica como string decimal exata
  (`123456789012345678.123456789`), sem passar por `f64`/`rust_decimal` — mesmo padrão do
  `NUMERIC` do Postgres (ADR 0003) e do `DECIMAL` do MySQL.
- **`DATE` inclui hora** (confirmado — mapeado como `CellValue::Timestamp`, não `Date`) e
  **`TIMESTAMP WITH TIME ZONE`**: ambos decodificam certo via `oracle::sql_type::Timestamp`.
- **`SET TRANSACTION READ ONLY`**: bloqueia escrita de verdade (`ORA-01456`), confirmando
  que o driver aplica CLAUDE.md §2 igual aos outros dialetos.
- **Bind nomeado repetido** (`:id + :id` com um valor só): confirma que a ordem de
  `bind_order` já produzida por `sql/params.rs` (ADR — ver `params.rs`, ordena por
  primeira ocorrência) bate exatamente com a ordenação posicional que o ODPI-C espera —
  não precisa de bind por nome (`execute_named`), o posicional (`&[&dyn ToSql]`) resolve
  sozinho.
- **Cancelamento**: `Connection::break_execution()` chamado de outra thread, sobre a
  mesma conexão (`Connection` é `Send + Sync`, garantido pelo próprio crate via
  `AssertSend`/`AssertSync`) — interrompe uma query em andamento em ~390ms, sem precisar
  de conexão auxiliar como Postgres/MySQL precisam.
- **`libclntsh` (Instant Client) carrega via `dlopen` em runtime, não em link-time** —
  buildar o projeto não exige Instant Client instalado; só conectar de verdade exige.

Comparação atualizada com a 0001, agora com dado real dos dois lados:

| | Rota B (0001): sidecar Python | Rota B (esta ADR): crate `oracle`/ODPI-C |
|---|---|---|
| Processo | subprocesso separado, protocolo JSON-lines stdin/stdout | nenhum — roda no mesmo processo Rust |
| Empacotamento | PyInstaller embutido, ~50 MB, checkpoint de empacotamento próprio | nenhum interpretador embutido |
| Dependência externa em runtime | nenhuma (Python embutido) | Oracle Instant Client instalado na máquina |
| Cancelamento | canal de cancelamento separado sobre o protocolo IPC (não testado na 0001) | `break_execution()` nativo, testado contra Oracle real (~390ms) |
| Maturidade da evidência | 0 de 9 checks K1–K9 executados | K2, K4, K5, K6 (cancelamento), K8 (sessão read-only) e a checagem de erro sanitizado executados contra Oracle real |

O argumento original da 0001 contra o crate ODPI-C — "anula o argumento de bundle único
que justificava considerar Rust puro" — continua válido como trade-off, mas passa a
competir com um trade-off equivalente do lado do sidecar: um interpretador Python
embutido via PyInstaller também não é "bundle único" no sentido leve original (~50 MB
citados na própria 0001, mais superfície de bug de um protocolo IPC ponta a ponta nunca
testado). Diante de dois desvios do ideal original, esta ADR escolhe o que já tem
evidência real de funcionar — a Instant Client é uma dependência de runtime padrão,
documentada, gratuita para download, e o próprio `CLAUDE.md` já assume "driver oficial da
Oracle, muito maduro" como critério de risco aceitável em ambas as rotas B cogitadas.

## Consequências

- `db/oracle.rs` implementa `Driver`/`Session` direto sobre o crate `oracle`, sem
  protocolo de sidecar, sem `tokio::process::Command`, sem harness de driver dummy
  (o item "2.5 — harness do sidecar com driver dummy" da 0001 fica sem efeito).
- O roadmap volta a não precisar do checkpoint de empacotamento PyInstaller que a 0001
  tinha antecipado para o item 4.5 — o empacotamento normal do Tauri (ADR 0005) já cobre
  o binário inteiro, Oracle incluso.
- **README.md** agora documenta o pré-requisito de Instant Client only quando o usuário
  for conectar em Oracle de verdade (o build do projeto em si não exige) — ver seção
  "Cadastrar a connection".
- `tnsnames.ora`/wallet **não foi validado** (ambiente sem esse arquivo disponível) — só
  EasyConnect. Documentado como limitação conhecida em `README.md` e
  `dev/oracle-manual-test.md`.
- Testes de driver Oracle continuam fora do pipeline automatizado (CLAUDE.md §9) —
  `dev/oracle-manual-test.md` é o roteiro de validação manual, já rodado uma vez nesta
  sessão com todos os checks passando.
- `spikes/oracle-probe/` (evidência da ADR 0001, crate `oracledb`) permanece no
  repositório como registro histórico do porquê a Rota A foi descartada — continua válido,
  não precisa refazer.
