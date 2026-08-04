# ADR 0001: Camada de acesso ao Oracle — Rota A (Rust puro) vs Rota B (sidecar Python)

- Status: substituída por ADR 0006 — o descarte da Rota A (`oracledb`, exige nightly)
  continua válido; a recomendação de Rota B = sidecar Python foi revista depois do
  follow-up K1–K9 (pendente aqui) ter sido executado contra um Oracle real
- Data: 2026-07-31

## Contexto

O CLAUDE.md (§3) deixa em aberto a escolha entre acessar Oracle via crate Rust puro
(`oracledb`, thin, sem Instant Client) ou via sidecar `python-oracledb`, e amarra essa
escolha a um spike de 1 dia com critério objetivo de decisão (§12, item 1), definido em
`PLAN.md` como uma tabela de checks P1–P3 (pré-checagens sem banco) e K1–K9 (checks contra
Oracle real, qualquer falha decide por Rota B).

Neste ambiente de execução não havia um Oracle real do usuário disponível, e — por decisão
explícita do usuário durante a execução — o teste contra um Oracle real via container
(Oracle XE) foi **pulado deliberadamente**, registrado abaixo como follow-up. Os checks
P1–P3, que não dependem de banco, foram executados de verdade.

## Evidência

### P1 — `oracledb` 0.9.1 compila em stable?

**Resultado: NÃO.** Reproduzido diretamente:

```
error[E0554]: `#![feature]` may not be used on the stable release channel
  --> asupersync-0.3.9/src/lib.rs:52:46
   |
52 | #![cfg_attr(feature = "nightly-outcome-try", feature(try_trait_v2))]
```

Causa raiz identificada no `Cargo.toml` publicado do `asupersync` 0.3.9: a feature
`nightly-outcome-try` está no conjunto `default` do crate (linha 171/197), e o `oracledb`
depende de `asupersync = "=0.3.9"` com `features = ["tls"]` **sem** `default-features =
false` — ou seja, qualquer consumidor de `oracledb` herda a exigência de nightly. Não é
contornável por flags de feature no lado do consumidor.

### P2 — o runtime `asupersync` coexiste com tokio no mesmo processo?

**Resultado: SIM, mas com uma condição arquitetural relevante.** `oracledb` não roda sobre
tokio — importa diretamente primitivas próprias do `asupersync` (`asupersync::net::TcpStream`,
`asupersync::runtime::{Runtime, RuntimeBuilder}`, reactor próprio). `RuntimeBuilder::block_on`
bloqueia a thread chamadora, então a única forma viável é rodar o runtime `asupersync` numa
**thread OS dedicada**, separada da(s) thread(s) do runtime tokio do Tauri.

Testado com um probe real (`spikes/oracle-probe/src/main.rs`, executado com
`cargo +nightly run`): uma tarefa tokio e um runtime `asupersync` (`RuntimeBuilder::new()
.worker_threads(1).build()`, rodando numa `std::thread` própria) executando concorrentemente
no mesmo processo, ambos completando sem panic nem deadlock:

```
[asupersync] tick 0
[tokio] tick 0
[asupersync] tick 1
[tokio] tick 1
...
P2: PASS — os dois runtimes coexistiram no mesmo processo sem panic/deadlock
```

Consequência para o design: a Rota A, se adotada, exige uma thread OS dedicada por processo
(ou um pool) para o runtime `asupersync`, e uma ponte explícita (canais) entre o mundo tokio
(Tauri, IPC) e essa thread — mais invasivo do que "só compilar com uma dependência a mais".
Isso reforça, e não enfraquece, a necessidade do `trait Driver` isolar completamente o
`oracle.rs` do resto da aplicação (CLAUDE.md §3), pois é ele quem vai abrigar essa ponte.

### P3 — existe API pública de cancelamento?

**Resultado: SIM.** Confirmado por leitura direta do código-fonte de `oracledb` 0.9.1:

- `Connection::cancel_handle(&self) -> Result<CancelHandle>`
- `cancel(&mut self, cx: &Cx) -> Result<()>` (assíncrono, duas variantes no código)
- `cancel_blocking(&mut self) -> Result<()>` (síncrono)
- um módulo `blocking` com `connect`/`cancel` síncronos próprios

Não foi testado se o cancelamento efetivamente interrompe uma query em andamento no
servidor Oracle (isso é K6, que depende de banco real — ver follow-up).

### K1–K9 — checks contra Oracle real

**Não executados neste ambiente.** Estava disponível a opção de subir um Oracle XE via
container (`gvenzl/oracle-xe:21-slim`) neste mesmo ambiente, o que teria dado evidência real
para K2–K9 (precisão de `NUMBER`, CLOB, datas com timezone, fetch limitado, bind de lista,
build reproduzível) — embora não para K1 (método de autenticação específico do ambiente de
produção do usuário, que só se valida contra o Oracle real de produção). O usuário optou
explicitamente por pular essa etapa nesta execução e tratá-la como melhoria futura.

## Decisão

**Rota B (sidecar `python-oracledb`, thin mode) é a recomendação vigente.**

Aplicando o critério objetivo do `PLAN.md`:

```
SE K1..K9 falha (ou não verificado) → ROTA B, sem discussão.
SENÃO SE P1 == "compila em stable" → ROTA A.
SENÃO (K's passam, mas exige nightly) → ROTA B é a recomendação padrão,
   Rota A só mediante aceite explícito das 4 condições de exceção.
```

P1 já falha por si só (não compila em stable), o que desqualifica a Rota A do caminho direto
independentemente do resultado de K1–K9. E como K1–K9 não foram executados, a via de exceção
("aceitar nightly por escrito") também não está disponível: ela exige que os checks de
correção (precisão numérica, CLOB, datas, cancelamento real) já estejam provados — o que
não é o caso aqui. Não há, portanto, base para escolher Rota A agora.

A Rota B não foi validada nesta execução (não há sidecar Python implementado nem testado),
mas é a rota que o CLAUDE.md já descreve como apoiada em driver oficial maduro
(`python-oracledb` thin), e é o destino padrão quando a Rota A é descartada.

**Terceira via registrada, não escolhida:** crate `oracle` 0.6.3 (kubo/rust-oracle, síncrono,
via ODPI-C) compila em stable e tem cancelamento síncrono nativo, mas exige Oracle Instant
Client instalado na máquina do usuário — o que anula o argumento original de bundle único
que justificava considerar Rust puro. Fica registrada como alternativa se a Rota B for
barrada por política de não embarcar Python.

## Consequências

- O roadmap muda conforme já antecipado em `PLAN.md` §1.2(i): o item "5 — driver Oracle"
  passa a incluir protocolo de sidecar (JSON-lines sobre stdin/stdout), spawn via
  `tokio::process::Command` (nunca `tauri-plugin-shell`, por §7/§11), health check, canal de
  cancelamento separado, e um checkpoint de empacotamento PyInstaller mais cedo (item 4.5,
  já traz esse cuidado).
- Inserir o item **2.5 — harness do sidecar com driver dummy**, logo após o guard (`sql/`),
  para provar a forma do `trait Session` contra um processo fora do processo antes do driver
  Oracle de verdade existir — conforme já recomendado em `PLAN.md`.
- **Follow-up explícito (ponto de melhoria registrado, não implementado agora):** rodar os
  checks K1–K9 contra um Oracle real — idealmente o Oracle de produção do usuário para K1
  (método de autenticação), e um Oracle XE em container para K2–K9. Enquanto isso não for
  feito, esta decisão deve ser tratada como **provisória quanto à Rota B especificamente**:
  é a rota mais segura dado o que se sabe hoje, mas a suíte completa de evidências do spike
  original não foi concluída. Revisitar antes do item 5 do roadmap (driver Oracle de
  verdade) começar a valer.
- O probe `spikes/oracle-probe/` fica no repositório como evidência (fora do workspace,
  `target/` ignorado), não como código de produção.
