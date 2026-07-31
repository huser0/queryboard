# Plano de implementação — queryboard

> Gerado a partir de `CLAUDE.MD`. Estado do repo na geração: vazio, sem git.

## Sumário executivo

Duas descobertas da checagem de bibliotecas mudam decisões do documento:

1. **O crate `oracledb` (0.9.1, 23/jul/2026) exige toolchain nightly** — usa `#![feature(try_trait_v2)]` através da dependência `asupersync` (runtime async próprio, pinado em `=0.3.9`, não é tokio). Tem ~4.4k downloads totais. Isso não invalida a Rota A, mas muda o spike: há critérios que se decidem antes de conectar em qualquer banco.
2. **`serde_yaml` está descontinuado desde março/2024** e `tauri-specta` estável ainda é v1 (Tauri v1). Ambos precisam de substitutos definidos agora, não depois.

O resto do documento é implementável como está. O plano abaixo assume Rota A até o spike dizer o contrário, e organiza o código para que a troca custe um módulo.

---

# CAMADA 1 — Arquitetura e sequenciamento

## 1.1 Decisões caras de reverter

### D1 — O trait `Driver` (e o `ValidatedSql` como cinto de segurança de tipo)

A assinatura precisa carregar três coisas que o documento exige mas que não cabem em `connect/execute_select/cancel` ingênuos: fetch limitado (§6), cancelamento (§6), e transação read-only com rollback (§2.2).

```rust
// src-tauri/src/db/driver.rs
#[async_trait]
pub trait Driver: Send + Sync {
    fn dialect(&self) -> Dialect;                     // consumido pelo guard e por params.rs
    fn placeholder_style(&self) -> PlaceholderStyle;  // $n | ? | :nome
    async fn connect(&self, cfg: &ConnectionConfig, secret: &SecretRef)
        -> Result<Box<dyn Session>, DbError>;
}

#[async_trait]
pub trait Session: Send {
    /// SET TRANSACTION READ ONLY + statement timeout de sessão. Sempre chamado após connect.
    async fn begin_read_only(&mut self, limits: &Limits) -> Result<(), DbError>;

    /// `sql` só é construível pelo guard. Busca no máximo max_rows+1 linhas do cursor;
    /// a linha extra é descartada e só serve para marcar `truncated`.
    async fn execute_select(
        &mut self,
        sql: &ValidatedSql,
        binds: &[Bind],
        limits: &Limits,               // { max_rows, timeout, fetch_size }
        cancel: CancellationToken,
    ) -> Result<ResultSet, DbError>;

    /// Sempre rollback, nunca commit. Consome a sessão.
    async fn rollback_and_close(self: Box<Self>) -> Result<(), DbError>;
}
```

Três pontos que valem mais que o resto da assinatura:

- **`ValidatedSql` é um newtype cujo construtor é privado ao módulo `sql::guard`.** É impossível, em nível de tipo, chamar um driver com SQL não validada. Nenhuma revisão de código precisa lembrar disso.
- **`cancel: CancellationToken` (tokio-util) entra desde o primeiro driver.** Cancelamento retrofitado obriga a mudar cinco camadas. Cada driver implementa a sua mecânica: Postgres via segunda conexão + `pg_cancel_backend(pid)` (pid capturado no `begin_read_only` com `SELECT pg_backend_pid()`); MySQL via `KILL QUERY <connection_id()>`; Oracle via a API de break do driver (é um dos checks do spike).
- **`Limits` entra no `execute_select`, não na connection.** O runner de flow precisa apertar o teto por passo depois.

**Como isso isola Rota A vs Rota B:** na Rota B, `oracle.rs` vira um cliente de um processo sidecar — `connect` faz spawn (ou pega do pool), `execute_select` serializa `{sql, binds, limits}` em JSON-lines no stdin e lê o `ResultSet` do stdout, `cancel` manda um frame de controle por um canal separado. O trait não muda; `ResultSet`, `CellValue` e `DbError` não mudam. **Condição: nenhum tipo do driver pode vazar.** Nada de `sqlx::PgRow` ou `oracledb::Row` acima de `db/`. `CellValue` é enum próprio.

> Trade-off: um trait mais gordo custa boilerplate por driver, mas é o que impede que fetch limitado, timeout e cancelamento virem gambiarra por dialeto.

**Alerta de escopo (§7 e §11):** se a Rota B ganhar, **não** use `tauri-plugin-shell` para spawn do sidecar — inicie do Rust com `tokio::process::Command`, sem expor capability de shell ao webview. Pelo mesmo motivo, **não use `tauri-plugin-keyring`**: expõe `getPassword` ao JS, violando "senha nunca volta pelo IPC" (§7). Use o crate `keyring` direto no Rust.

### D2 — Parsing SQL para o `guard.rs`

**Recomendação: `sqlparser` 0.62 (apache/datafusion-sqlparser-rs), com a feature `visitor` ligada.**

Verificado: v0.62.0 de 07/mai/2026, ~10,8M downloads recentes, governança Apache, e **existe `OracleDialect`** junto de `PostgreSqlDialect` e `MySqlDialect` — os três bancos do projeto têm dialeto de primeira classe.

| Requisito (§9) | Como o sqlparser resolve |
|---|---|
| Statements empilhados | `parse_statements()` devolve `Vec<Statement>`; exigir `len() == 1`. Reforçado por contagem de tokens `SemiColon` antes do parse. |
| Comentários escondendo comando | O tokenizer trata comentário como token; `SELECT 1; -- \n DROP` continua sendo 2 statements. |
| `;` dentro de string literal | O tokenizer distingue `SingleQuotedString` de `SemiColon`. Regex jamais acertaria — é a razão principal de usar um parser de verdade. |
| DML dentro de CTE | O AST modela `Cte { query: Box<Query> }`. Mas `SetExpr` tem variantes `Insert` e `Update` — **é obrigatório rejeitá-las explicitamente no walk**, senão é aqui que passa o bypass. |
| Hints `/*+ ... */` | São comentários para o tokenizer. **Decisão associada: nunca reserializar o AST.** O que vai ao driver é a string original validada, com apenas os placeholders reescritos por splice de offsets. |
| Homóglifos unicode | O sqlparser **não** resolve e não deve resolver. Etapa anterior ao parse. |

**Homóglifos e invisíveis são um estágio pré-parse, textual:**
1. Rejeitar `NUL`, controles fora de `\t\n\r`, e todos os controles bidi/invisíveis (`U+200B..U+200F`, `U+202A..U+202E`, `U+2066..U+2069`, `U+FEFF`) em **qualquer** posição, inclusive dentro de literais e comentários (Trojan Source).
2. Tokenizar e, para todo token que **não** seja string literal nem identificador entre aspas, exigir ASCII. Um `SELECT` com `Е` cirílico não parseia mesmo — mas a regra explícita barra o caso inverso.

Descartados: tokenizer próprio (custo alto no arquivo onde não se pode errar); só regex (não distingue literal de comando); sqlparser + reserialização (perde hints).

> Trade-off: sqlparser cobre um subconjunto do PL/SQL e vai rejeitar SQL Oracle exótica válida; falso positivo é explicitamente aceitável pelo §2.

### D3 — Runtime CEL

**Recomendação: crate `cel` 0.14.1 (cel-rust/cel-rust), atrás de um adapter próprio.**

Verificado: publicado em 27/jul/2026, ~437k downloads recentes, continuação renomeada de `cel-interpreter`, com palestra na FOSDEM 2026. É a única implementação Rust de CEL com adoção real. Não é a de referência do Google (Go/C++/Java), então há lacunas de conformidade — mas o uso aqui é um subconjunto pequeno.

O que precisa ser construído por cima, em `rules/engine.rs`:
- **Sem timeout/limite de execução embutido.** CEL não tem loops, então o custo só explode com listas grandes; limitar entradas (teto de 1000 linhas do §5), rodar em `spawn_blocking` com watchdog, e rejeitar no save expressões acima de N nós de AST / profundidade.
- **Sem validação contra schema.** A validação no editor (colunas existentes, `origin` só com `correlate_on`) é código nosso.
- Compile-once: `Program::compile` uma vez por regra, cachear; nunca compilar por linha.

Não expor nada com relógio — `now()` não existe em CEL padrão e não deve ser adicionado (§5, pureza).

**Adapter obrigatório.** `rules/engine.rs` expõe tipos próprios (`CompiledRule`, `EvalContext`, `EvalError`); nenhum tipo do crate `cel` sai do módulo. Se precisar trocar: (a) avaliador próprio para o subconjunto usado — viável, porque é comparações, `&&`/`||`, acesso a campo, indexação e funções puras; (b) sidecar com `cel-go`. **Não** usar `rhai`/`lua`: têm loops e funções de usuário, proibidos pelo §11.

O `message` com `{{row.offer_id}}` **não é CEL** e não deve virar uma segunda linguagem: extrair cada `{{ ... }}`, compilar o miolo como CEL, usar o mesmo engine.

> Trade-off: `cel` é jovem, mas qualquer alternativa ou reintroduz Turing-completude (proibida) ou custa um interpretador à mão.

### D4 — Modelo no SQLite e serialização YAML

**Recomendação: híbrido — relacional para identidade/índice, colunas JSON para as partes aninhadas; um único modelo serde que serve tanto ao JSON no banco quanto ao YAML de export.**

O ponto que trava o desenho é o §7: "export não inclui credenciais — apenas o identificador lógico da connection". Logo, o `slug` precisa ser a chave estrangeira real, não o uuid.

```sql
CREATE TABLE connection (
  id TEXT PRIMARY KEY,                       -- uuid interno
  slug TEXT NOT NULL UNIQUE,                 -- 'erp_prod' — é o que aparece no YAML
  name TEXT NOT NULL,
  kind TEXT NOT NULL CHECK (kind IN ('oracle','postgres','mysql')),
  host TEXT NOT NULL, port INTEGER NOT NULL,
  database TEXT, service_name TEXT,
  username TEXT NOT NULL,
  -- SEM coluna de senha. keyring: service='queryboard', account=connection.id
  options_json TEXT NOT NULL DEFAULT '{}',
  max_rows INTEGER NOT NULL DEFAULT 1000,
  timeout_ms INTEGER NOT NULL DEFAULT 30000,
  created_at TEXT NOT NULL, updated_at TEXT NOT NULL
);

CREATE TABLE query (
  id TEXT PRIMARY KEY,
  slug TEXT NOT NULL UNIQUE,                 -- 'consulta_oferta'
  name TEXT NOT NULL,
  connection_slug TEXT NOT NULL REFERENCES connection(slug) ON UPDATE CASCADE,
  sql TEXT NOT NULL,
  params_json TEXT NOT NULL DEFAULT '[]',    -- [{name,type,required}]
  columns_cache_json TEXT,                   -- metadados de coluna da última execução ok
  created_at TEXT NOT NULL, updated_at TEXT NOT NULL
);

CREATE TABLE rule (
  id TEXT PRIMARY KEY,
  slug TEXT NOT NULL UNIQUE,                 -- 'oferta_decorrendo'
  query_slug TEXT NOT NULL REFERENCES query(slug) ON UPDATE CASCADE,
  scope TEXT NOT NULL CHECK (scope IN ('row','result')),
  subject_column TEXT,                       -- ver Q4 (veredito 'unmatched')
  priority INTEGER NOT NULL DEFAULT 0,
  when_expr TEXT NOT NULL,
  then_json TEXT NOT NULL,                   -- {severity,title,message}
  builder_json TEXT,                         -- estado do builder visual, se veio dele
  enabled INTEGER NOT NULL DEFAULT 1,
  created_at TEXT NOT NULL, updated_at TEXT NOT NULL
);
CREATE INDEX rule_by_query ON rule(query_slug, priority DESC, slug);

CREATE TABLE flow (
  id TEXT PRIMARY KEY,
  slug TEXT NOT NULL UNIQUE,
  name TEXT NOT NULL,
  inputs_json TEXT NOT NULL DEFAULT '[]',
  steps_json TEXT NOT NULL,                  -- [{step,query,depends_on,bind{},correlate_on,run_if,on_empty}]
  created_at TEXT NOT NULL, updated_at TEXT NOT NULL
);

CREATE TABLE app_setting (key TEXT PRIMARY KEY, value TEXT NOT NULL);
```

Por que não normalizar `steps` e `bind` em tabelas: é app local mono-usuário, ninguém vai fazer query analítica sobre passos de flow; normalizar cria uma camada de mapeamento que só existe para ser desfeita no export. `columns_cache_json` guarda **metadados de coluna**, nunca linhas — não conflita com o §11 ("sem cache de resultados"). É o que torna o preview e a validação de regras possíveis sem inventar um `DESCRIBE` por dialeto; vale existir desde o item 3.

Export YAML — bundle único, sem credenciais:

```yaml
version: 1
kind: queryboard.bundle
connections:
  - slug: erp_prod
    kind: oracle          # só o contrato lógico; host/user/senha ficam de fora
queries:
  - slug: consulta_oferta
    connection: erp_prod
    params: [{ name: offer_id, type: number }]
    sql: |
      SELECT o.offer_id, o.offer_status, o.start_date
      FROM tb_offer o WHERE o.offer_id = :offer_id
rules:
  - id: oferta_decorrendo
    query: consulta_oferta
    scope: row
    priority: 100
    when: "row.offer_status == 5"
    then: { severity: info, title: "Oferta decorrendo", message: "Oferta {{row.offer_id}} ..." }
flows:
  - slug: investiga_oferta
    inputs: [{ name: offer_id, type: number }]
    steps:
      - step: consulta_oferta
      - step: consulta_envio_loja
        depends_on: [consulta_oferta]
        run_if: "steps.consulta_oferta.rows[0].offer_status == 5"
```

**Crate YAML: `serde_yaml_ng`** (continuação independente do `serde_yaml` de dtolnay, API idêntica, mantida). Alternativa pura-Rust sem `unsafe-libyaml`: `serde-saphyr`. **Não** use `serde_yaml` (descontinuado em mar/2024) nem `serde_yml` (fork de procedência discutível).

> Trade-off: JSON aninhado no SQLite abre mão de integridade referencial dentro de flows/rules, em troca de round-trip YAML↔struct trivial e de um único modelo serde canônico.

### D5 — `max_rows` via fetch limitado (e o que isso impõe ao Driver)

"`max_rows` via fetch limitado, nunca injetando `LIMIT`" tem consequência direta na assinatura: **`execute_select` não pode ser "roda e devolve tudo"** e o resultado precisa carregar `truncated: bool`.

- **Postgres (`sqlx` 0.9.0):** `query.fetch(&mut conn)` devolve um `Stream`; consumir com `.take(max_rows + 1)`. A linha extra é o detector de truncamento. Plano B documentado, se o volume na rede pesar: `DECLARE cur NO SCROLL CURSOR FOR <sql>` + `FETCH FORWARD n` dentro da transação read-only — não é injetar `LIMIT`, é envelopar; só faça se medir necessidade.
- **MySQL (`sqlx`):** mesmo padrão de stream.
- **Oracle:** fetch em lotes com `arraysize = min(max_rows + 1, 500)`, parando e fechando o cursor. É um dos checks do spike.

`fetch_size` fica em `Limits` porque o valor certo difere entre "grade da UI" (100) e "passo intermediário que alimenta um `collect`" (1000).

### D6 — Fidelidade numérica no IPC

`NUMBER(38,10)` do Oracle não cabe em `f64`, e JSON no webview vira `double` silenciosamente. Sem decidir isso agora, a regra mais valiosa do produto (`row.preco != origin.preco`, §6.5) dá falso positivo por ponto flutuante.

- `CellValue::Decimal(String)` — string decimal **normalizada canonicamente** (sem zeros à esquerda, escala fixa por coluna). Nunca passa por `f64`.
- No IPC/TS: `string`; o front nunca faz `Number()` nela; renderiza monoespaçada à direita.
- No CEL: `NUMBER` com escala 0 que cabe em `i64` → `int` (faz `row.offer_status == 5` funcionar como o §5 escreve); qualquer outro numérico → string decimal normalizada. Como a normalização é a mesma dos dois lados, `row.preco != origin.preco` compara strings e é **exato**.

Visível ao usuário: documentar no editor de regras, junto do aviso de minúsculas (§5).

### D7 — Tipos IPC Rust↔TS

`tauri-specta` estável é a 1.0.2 (mai/2023, Tauri **v1**); a linha v2 só existe em `-rc`. **Recomendação: `ts-rs` 12.0.1** (jan/2026) gerando `src/ipc/types.ts` a partir dos structs Rust, mais um wrapper `src/ipc/client.ts` escrito à mão sobre `invoke`. Job de CI que regera e falha se divergir do commitado — é o que transforma "divergência é bug" (§8) em algo verificável.

---

## 1.2 Leitura crítica do roadmap (§12)

### Onde concordo

- **Item 1 (spike Oracle) primeiro — fortemente.** É a única decisão capaz de invalidar toda a camada de banco, barata de tomar e cara de adiar. Acrescento: crate descartável fora da árvore do produto, e ADR escrito (senão a decisão evapora).
- **Item 2 (guard) antes do 3 (driver).** O guard é puro, não depende de banco, é 100% testável em segundos, e é pré-requisito de qualquer chamada a driver.
- **Item 5 (Oracle) antes do 6 (flow).** Flow multi-banco sem Oracle é flow de mentira para este usuário.
- **Item 13 (MySQL) tarde**, com ressalva: a tabela de expansão de listas do §6.4 tem que estar desenhada nos três dialetos em `params.rs` desde o começo, senão a API é moldada para dois dialetos e entortada no terceiro.

### Onde discordo

**(a) Faltam pré-requisitos escondidos do item 4.** "Cadastro de connection" exige `store/` (SQLite + migrations), `secrets.rs` (keyring) e o **sanitizador de erros** do §7. Se o sanitizador não existir antes da primeira mensagem de erro chegar à UI, vira retrofit e algum caminho vaza DSN. → item **3.5** explícito entre 3 e 4.

**(b) `params.rs` não aparece no roadmap, mas o item 3 diz "com parâmetros".** `params.rs` e `guard.rs` operam sobre o **mesmo texto cru** e precisam compartilhar tokenizer e offsets. Separados, viram duas varreduras divergentes — o cenário em que um bypass nasce. → mover `params.rs` para dentro do item 2.

**(c) Cancelamento não é item nenhum, mas é requisito do §6.** Se o `CancellationToken` não estiver no trait desde o item 3, será retrofitado por UI, IPC, runner e três drivers. → caminho de cancelamento ponta a ponta fechado no item 4, com uma query só, antes de existir flow.

**(d) Item 7 (rules) deveria ser dividido, e a primeira metade vem antes do item 6.** Regras `scope: row`/`result` sobre **uma única query** não precisam de flow — só de um `ResultSet`, que existe ao fim do item 4.
- **7a** — engine CEL, `scope: row`/`result`, contextos `row`/`result`/`params`, badges, veredito `unmatched`. **Antes do item 6.**
- **7b** — contextos `steps.*` e `origin.*`. Depois do 6 e do 10.

Motivos: entrega o valor central do produto meses antes; e o crate `cel` é o segundo maior risco externo, com a forma do contexto de avaliação gravada na saída do runner — validar **antes** de escrever o runner é muito mais barato.

**(e) `run_if` (item 8) deveria estar dentro do item 6.** Gating não é propagação de dado, é máquina de estados. Os `skipped` com motivo e a cascata por dependência falha são a mesma engrenagem. → item 6 entrega a máquina de estados completa (`pending|running|ok|empty|skipped|error|timeout|cancelled`, motivo do skip, cascata, cancelamento); item 8 fica só com `first`.

**(f) Item 9 (`collect` + listas) antes do item 8 (`first`).** O próprio documento diz que `collect` é o caso mais comum e o que destrava o caso PLU+LOJA → processo — maior valor do roadmap. Suas dependências são `params.rs` + um driver + o runner; não depende de `first`, nem de regras, nem de correlação. E o lote de 1000 do Oracle é a parte mais arriscada do runner, merece exposição cedo.

**(g) Item 14 (export YAML) tarde — parcialmente.** A **UX** pode ficar no 14. O **modelo serde canônico** (com teste de round-trip YAML↔struct↔JSON) tem que existir nos itens 6 e 7a, porque é ele que é gravado em `steps_json`/`then_json`. Definir só no 14 significa migração de dados de todo mundo que já usou a ferramenta.

**(h) Falta um checkpoint de empacotamento.** Assinatura Windows, notarização macOS e (na Rota B) PyInstaller + falso positivo de antivírus aparecem tarde e custam dias. → gerar o instalador no CI logo depois do item 4, com walking skeleton, e instalar de fato na máquina alvo.

**(i) Se o spike escolher Rota B, o roadmap muda.** O item 5 vira "protocolo de sidecar + spawn + health + cancel + empacotamento + driver Oracle". Nesse caso, inserir **item 2.5: harness do sidecar validado com driver dummy** logo após o guard — out-of-process muda a forma do cancelamento e do streaming.

### Roadmap recomendado

`0` bootstrap · `1` spike Oracle · `2` guard + params · *(2.5 harness sidecar, só se Rota B)* · `3` driver Postgres · `3.5` store + keyring + IPC + sanitizador · `4` UI mínima + cancelamento ponta a ponta · `4.5` checkpoint de empacotamento no CI · `7a` rules em query única · `5` driver Oracle · `6` flow (máquina de estados completa, com `run_if`) · `9` listas + `collect` + lotes Oracle · `8` `first` · `10` `correlate_on` + `origin` + `7b` · `11` editor visual · `12` `fan_out` · `13` MySQL · `14` export/import YAML · `15` CSV + histórico.

---

## 1.3 Riscos, em ordem

| # | Risco | Impacto | Mitigação |
|---|---|---|---|
| R1 | `oracledb` 0.9.1 exige **nightly** (`try_trait_v2` via `asupersync`), runtime async próprio pinado, ~4.4k downloads | Rota A pode ser inviável para app distribuído; nightly quebra sem aviso | Spike com checks de kill definidos; trait `Driver` isolando; Rota B pronta; terceira opção: crate `oracle` 0.6.3 (kubo, ODPI-C, síncrono, maduro) — mas exige Instant Client instalado, o que anula o argumento de bundle |
| R2 | Falso negativo no `guard.rs` → escrita em produção | Catastrófico, irreversível | Defesa em camadas (§2); `ValidatedSql` newtype privado; suíte de bypass obrigatória; `cargo-fuzz`; transação read-only sempre; introspecção de privilégios com banner de aviso |
| R3 | `cel` não é a implementação de referência; sem limites de execução | Regras erradas ou painel travado | Adapter isolando o crate; testes golden do subconjunto; limite de nós/profundidade no save; `spawn_blocking` com watchdog; teto de 1000 linhas |
| R4 | Fidelidade de `NUMBER(38,x)`, `CLOB`, `TIMESTAMP WITH TZ` até o CEL e o webview | A regra de divergência de preço dá resultado errado | D6: decimal como string canônica ponta a ponta; testes de tipo por driver desde o item 3; nunca `f64` |
| R5 | Cancelamento/timeout desigual entre drivers | Flow não cancelável (viola §6); cursor pendurado | `CancellationToken` no trait desde o item 3; mecânica por dialeto; check obrigatório no spike; `statement_timeout`/`max_execution_time` como rede de segurança |
| R6 | Empacotamento: assinatura, notarização, antivírus (crítico na Rota B) | Bloqueia entrega, sempre no fim | Checkpoint 4.5, instalador gerado e instalado de verdade cedo |
| R7 | `ORDER BY` + lotes de 1000 no Oracle: concatenar lotes **quebra a ordenação global** | Resultado silenciosamente errado — o que a ferramenta existe para evitar | Guard já parseia o AST: detectar `ORDER BY` raiz e avisar explicitamente no detalhe da execução; v2 reordena no cliente quando as chaves são colunas simples. **Lacuna do §6.4** |
| R8 | Produto cartesiano em `correlate_on` com chave duplicada dos dois lados | Painel explode; §9 exige detectar e avisar | Índice de correlação com contagem por chave; detecção antes de materializar; desenhar a estrutura no item 6, mesmo com uso no 10 |
| R9 | Divergência de tipos entre IPC Rust e TS | §8 chama de bug | `ts-rs` 12 gerando `types.ts` + job de CI que falha em diff |
| R10 | Vazamento de credencial em log/erro | Viola §7 | Sanitizador antes de qualquer erro subir; `Debug` customizado nos structs de config; teste que injeta um DSN completo numa mensagem de erro e afirma que nada sobrevive |

---

# CAMADA 2 — Passo a passo (itens 0 a 4)

## Passo 0 — Bootstrap

1. `git init`; branch `main`; `.gitignore` (node_modules, dist, target, `*.db`, `.env`), `.editorconfig`.
2. Scaffold: `pnpm create tauri-app@latest` na pasta atual, template **React + TypeScript (Vite)**, gerenciador **pnpm**. Tauri v2 (linha 2.10.x). Preservar o `CLAUDE.MD` existente.
3. Fixar toolchains: `rust-toolchain.toml` com canal `stable` e versão explícita; `.nvmrc` / `packageManager` no `package.json`.
4. Reorganizar `src-tauri/src` para a estrutura do §4 — `ipc/`, `db/`, `sql/`, `rules/`, `flow/`, `store/`, `secrets.rs` declarados em `lib.rs`. **`main.rs` só chama `lib::run()`.** Regra: só `ipc/` conhece `tauri::`; o resto é testável sem GUI.
5. `tsconfig.json` com `strict: true`, `noUncheckedIndexedAccess: true`, e ESLint proibindo `any`.
6. `tauri.conf.json`: CSP `default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; connect-src 'self'`; sem `shell`, sem `fs` amplo, sem `http` nas capabilities. Nenhum asset remoto.
7. CI (`.github/workflows/ci.yml`): `fmt` (`cargo fmt --check`), `clippy` (`-D warnings`), `test` (`cargo test`), `front` (`pnpm tsc --noEmit && pnpm test && pnpm lint`), `ipc-types` (regera com ts-rs e falha em diff). Matriz ubuntu/windows/macos no build. Docker só no job de integração, opcional.
8. `commitlint` + Conventional Commits.
9. `docs/adr/` com `0000-template.md`.

**Pronto quando:** `pnpm tauri dev` abre a janela; `pnpm tauri build` produz binário; os cinco jobs de CI passam num PR vazio; `cargo test` roda sem Docker e sem rede.

**Testes:** um teste unitário trivial no core e um `vitest` trivial no front, só para provar que ambos os pipelines executam.

---

## Passo 1 — Spike Oracle (timebox 1 dia)

Fora da árvore do produto: `spikes/oracle-probe/` (crate binário independente, **não** no workspace). Nenhuma linha vai para produção.

### Pré-checagens (0–60 min) — antes de tocar em banco

| # | Check | Como | Resultado |
|---|---|---|---|
| P1 | `oracledb` 0.9.1 compila em **stable**? | `cargo build` com toolchain stable | Já verificado como **não** (exige nightly). Confirmar se mudou na 0.9.x |
| P2 | O runtime `asupersync` coexiste com o tokio do Tauri no mesmo processo? | Binário mínimo com runtime tokio ativo + conexão `oracledb` | Se exigir thread dedicada, medir o custo |
| P3 | Existe API pública de cancelamento/break? | docs.rs + código | Sim/Não, com o nome do método |

**Regra de parada seca:** se em 2h não houver `SELECT 1 FROM dual` retornando, pare e vá para a Rota B. Não gaste o dia depurando o handshake.

### Checks de kill (2h–7h) — qualquer falha decide

| # | Check | Prova | Falha significa |
|---|---|---|---|
| K1 | **Autenticação real do usuário** em thin mode | Conectar com o método que o ambiente usa: senha (O5LOGON), wallet TCPS, ou token | Se o ambiente usa autenticação externa/OS ou bequeath, thin **não suporta** → Rota B imediata |
| K2 | **`NUMBER` de precisão alta exato** | `SELECT CAST(1234567890123456789012345678.12345 AS NUMBER) FROM dual`, comparar a **string** devolvida com a esperada; idem `NUMBER(38,10)` e `NUMBER` sem precisão | Se passar por `f64` em qualquer ponto → Rota B |
| K3 | **CLOB > 32 KB** íntegro | Coluna CLOB real, comparar tamanho e hash | Truncou → Rota B |
| K4 | **Datas** | `DATE` (hora preservada), `TIMESTAMP`, `TIMESTAMP WITH TIME ZONE`, `WITH LOCAL TIME ZONE` — offset e precisão corretos | Perda de hora ou offset → Rota B |
| K5 | **`SET TRANSACTION READ ONLY` aceito**, sem commit implícito na saída | Executar; confirmar que `rollback` no fim não erra. **Não** tentar DML no Oracle do usuário; se houver schema sandbox descartável, validar lá; senão, credencial `SELECT`-only cobre | Statement rejeitado pelo driver → Rota B |
| K6 | **Cancelamento** de query em andamento | Query lenta (produto cartesiano sobre `all_objects`), cancelar após 2s, confirmar em `v$session`/`v$sql` que **parou no servidor**. Medir o tempo até parar | Sem cancelamento confiável em < timeout → Rota B (requisito do §6, sem workaround aceitável) |
| K7 | **Fetch limitado** | Tabela grande; `arraysize` pequeno; parar após 1001 linhas; medir tempo e RSS | Se materializar tudo → Rota B |
| K8 | **Bind de lista** | `:p0..:p999` com 1000 → funciona; 1001 → `ORA-01795` | Comportamento diferente exige redesenho do `params.rs` — registrar, não é kill |
| K9 | **Build no SO alvo** reproduzível no CI | Mesmo build no runner | Falha → Rota B |

### Critério objetivo de decisão

```
SE K1 falha OU K2 falha OU K3 falha OU K4 falha OU K6 falha OU K7 falha OU K9 falha
   → ROTA B (sidecar Python), sem discussão.

SENÃO SE P1 == "compila em stable"
   → ROTA A.

SENÃO (todos os K passam, mas exige nightly)
   → ROTA B é a recomendação padrão: um app desktop distribuído
     não deve depender de nightly pinado, e o crate tem ~4.4k downloads.
     Rota A só se o time aceitar, por escrito no ADR:
       (i) rust-toolchain.toml pinado numa data específica de nightly,
       (ii) build reproduzível no CI com essa data,
       (iii) dono nomeado para revalidar a cada bump,
       (iv) plano de migração para Rota B mantido vivo.
```

**Terceira via, só se a Rota B for barrada por tamanho de bundle ou por política de não embarcar Python:** crate `oracle` 0.6.3 (kubo/rust-oracle, ODPI-C, síncrono, maduro, mas exige Oracle Instant Client instalado na máquina do usuário). Registrar no ADR como opção, não escolher sem necessidade.

**Pronto quando** existe `docs/adr/0001-camada-oracle.md` com: tabela K1–K9 preenchida com pass/fail e evidência (saída real, não "funcionou"), a decisão, e as consequências para o roadmap. **Spike sem ADR escrito é spike perdido.**

**Testes:** nenhum — é spike. As evidências vão para o ADR; o código é descartado.

---

## Passo 2 — `guard.rs` + `params.rs` + suíte de bypass

```
src-tauri/src/sql/mod.rs           # ValidatedSql (construtor privado ao módulo), Dialect
src-tauri/src/sql/guard.rs         # pipeline de validação
src-tauri/src/sql/lexical.rs       # pré-checagens unicode + contagem de statements
src-tauri/src/sql/denylist.rs      # funções proibidas por dialeto
src-tauri/src/sql/params.rs        # extração de :nome, reescrita por dialeto, listas
src-tauri/tests/guard_bypass.rs
src-tauri/tests/guard_valid.rs
src-tauri/tests/params.rs
src-tauri/fuzz/fuzz_targets/guard.rs
docs/adr/0002-parser-sql.md
```

**Pipeline do guard (7 estágios, cada um com variante própria de erro `thiserror`)**

1. **Bytes:** tamanho máx. 256 KB; rejeitar `NUL`, controles fora de `\t\n\r`, e bidi/invisíveis (`U+200B..200F`, `U+202A..202E`, `U+2066..2069`, `U+FEFF`) em qualquer posição.
2. **Tokenizar** (`sqlparser::tokenizer`) e exigir ASCII em todo token que não seja string literal nem identificador entre aspas.
3. **Contagem de statements:** tokens `SemiColon`; no máximo um, e só se seguido de whitespace/EOF; removê-lo do texto final.
4. **Parse** com o dialeto da connection, `with_recursion_limit(50)`. Exigir `stmts.len() == 1`.
5. **Raiz** deve ser `Statement::Query`.
6. **Walk com `Visitor`** (usar o visitor, não `match` manual, para que uma variante nova de AST numa versão futura não abra buraco silencioso). Rejeitar:
   - `SetExpr::Insert` e `SetExpr::Update` — **este é o bypass de DML em CTE**;
   - `Select.into` (o `SELECT ... INTO tabela` do Postgres cria tabela);
   - `Query.locks` não vazio (`FOR UPDATE` / `FOR SHARE`);
   - `TableFactor::Function` / table functions (`TABLE(...)`) — ver Q2;
   - `Expr::Function` cujo nome normalizado bata na denylist do dialeto.
7. **Produzir `ValidatedSql`** com o **texto original** (nunca reserializado), preservando hints e comentários.

**Denylist inicial** — Postgres: `pg_read_file`, `pg_read_binary_file`, `pg_ls_dir`, `pg_stat_file`, `lo_import`, `lo_export`, `dblink*`, `nextval`, `setval`, `pg_sleep`, `pg_terminate_backend`, `pg_cancel_backend`, `query_to_xml`. Oracle: qualquer `dbms_*`, `utl_*`, `owa_*`, `sys.*`, `httpuritype`, `dbms_xmlgen`. MySQL: `load_file`, `sleep`, `benchmark`, `get_lock`, `release_lock`, `master_pos_wait`.

**`params.rs`** — reusa o tokenizer do guard (mesma varredura, mesmos offsets):
- Extrai `:nome` fora de literais e comentários; trata `::` do Postgres como cast, não parâmetro.
- Reescreve por **splice de offsets de byte** no texto original: Postgres `$n`, MySQL `?`, Oracle mantém `:nome`.
- Listas: só é aceito o padrão `IN (:param)` quando `param` é `list<...>`. Postgres → `= ANY($n)` com array nativo; MySQL → `IN (?, ?, ...)`; Oracle → `IN (:p0, ..., :pN)`, com lote de 1000. Parâmetro de lista fora desse padrão → erro claro no save.
- **Após a reescrita, roda o guard de novo** sobre o texto reescrito. Custa microssegundos e fecha a classe inteira de "a expansão de parâmetro introduziu algo".

**Pronto quando**
- Todos os casos de bypass abaixo rejeitados e todos os legítimos aceitos.
- `cargo fuzz run guard -- -max_total_time=300` sem crash e sem pânico.
- Cobertura de linha de `guard.rs` + `lexical.rs` ≥ 95% (`cargo llvm-cov`).
- `clippy -D warnings` limpo; nenhum `unwrap()` fora de teste.
- `ValidatedSql` sem construtor público (teste em `tests/` que só compila via `guard::validate`).

**Bypass (devem ser REJEITADOS)**

`SELECT 1; DROP TABLE t` · `SELECT 1; -- \n DROP TABLE t` · `SELECT 1 /* ; DROP TABLE t */ FROM dual` seguido de segundo statement · `WITH x AS (INSERT INTO t VALUES (1) RETURNING *) SELECT * FROM x` · `WITH x AS (DELETE FROM t RETURNING *) SELECT * FROM x` · `WITH x AS (UPDATE t SET a=1 RETURNING *) SELECT * FROM x` · `SELECT * FROM t FOR UPDATE` · `SELECT * INTO nova FROM t` · `CREATE TABLE x AS SELECT 1` · `BEGIN NULL; END;` · `DECLARE v NUMBER; BEGIN NULL; END;` · `DO $$ BEGIN END $$` · `CALL p()` · `EXEC p` · `MERGE INTO t USING ...` · `TRUNCATE TABLE t` · `GRANT SELECT ON t TO u` · `SELECT` com `Е` cirílico · identificador com `U+200B` no meio · qualquer SQL contendo `U+202E` · `SELECT pg_read_file('/etc/passwd')` · `SELECT dbms_lock.sleep(10) FROM dual` · `SELECT load_file('/etc/passwd')` · `SELECT nextval('s')` · 1000 parênteses aninhados (erro controlado, **não** stack overflow) · SQL vazia · SQL só com comentário.

**Falsos positivos (devem ser ACEITOS)**

`SELECT 'a;b' FROM dual` · `SELECT 1;` (ponto e vírgula final único, normalizado) · `SELECT /*+ FULL(t) */ * FROM t` **com o hint preservado byte a byte no `ValidatedSql`** · `SELECT 'ação' FROM dual` · `WITH RECURSIVE ...` legítimo · `UNION ALL` · subquery em `FROM`, `IN`, `EXISTS` e `SELECT` · `SELECT "Coluna Com Espaço" FROM t`.

**Testes de `params.rs`:** lista vazia (marca `empty`, nunca gera `IN ()`), 1 elemento, exatamente 1000, 1001 (dois lotes no Oracle), 10.001 (estouro do teto), duplicados com `distinct`, nulos com e sem `drop_nulls`, tipos mistos na coluna, `::text` não confundido com parâmetro, `:nome` dentro de string literal ignorado, parâmetro escalar em posição de lista e vice-versa.

---

## Passo 3 — Driver Postgres + query única parametrizada

```
src-tauri/src/db/mod.rs
src-tauri/src/db/driver.rs      # trait Driver + Session, Limits, Bind
src-tauri/src/db/value.rs       # CellValue, ResultSet, ColumnMeta
src-tauri/src/db/error.rs       # DbError + sanitizador (§7)
src-tauri/src/db/postgres.rs
src-tauri/tests/pg_types.rs     # feature "integration", testcontainers
docs/adr/0003-driver-trait.md
docs/adr/0004-representacao-numerica.md
```

1. `driver.rs` conforme D1 e `value.rs` conforme D6. `CellValue`: `Null | Bool | Int(i64) | Decimal(String) | Float(f64) | Text(String) | Bytes(Vec<u8>) | Date | Time | Timestamp | TimestampTz | Interval(String) | Json(String) | Lob{...}`. `ColumnMeta`: nome original, nome normalizado minúsculo, tipo declarado, nullable.
2. `postgres.rs` com `sqlx` 0.9.0 (features `postgres`, `runtime-tokio`, `tls-rustls`, `chrono`, `rust_decimal`, `uuid`, `json`).
3. `begin_read_only`: `BEGIN` → `SET TRANSACTION READ ONLY` → `SET LOCAL statement_timeout = <ms>` → guardar `pg_backend_pid()`. Encerramento **sempre** `ROLLBACK`.
4. `execute_select`: stream com `.take(max_rows + 1)`; a linha extra vira `truncated = true` e é descartada. Respeitar `cancel` a cada iteração e, ao cancelar, abrir conexão auxiliar e chamar `pg_cancel_backend(pid)`.
5. **Sanitizador de erros** (`error.rs`): recebe o erro cru e devolve um `DbError` público sem host, porta, usuário, senha ou DSN, preservando código de erro do banco, `SQLSTATE` e a mensagem sintática. `Debug`/`Display` de `ConnectionConfig` e `SecretRef` reescritos para nunca imprimir segredo.
6. Introspecção de privilégios: query de catálogo, read-only, que descobre se o usuário tem privilégio de escrita, para o banner de aviso do §2.3.

**Pronto quando**
- `SELECT * FROM generate_series(1,100000) g` com `max_rows = 1000` devolve 1000 linhas, `truncated = true`, em < 2s, RSS estável.
- Query lenta (produto cartesiano — **não** `pg_sleep`, bloqueado pela denylist) cancelada em < 1s, backend some de `pg_stat_activity`.
- Timeout de 30s dispara como `DbError::Timeout`, não erro genérico.
- Erro de sintaxe chega sem nenhum fragmento do DSN — verificado por teste.
- Toda chamada de driver aceita apenas `&ValidatedSql`.

**Testes**
- **Tipos** (testcontainers, Postgres 16), asserção sobre a `String` e não sobre `f64`: `numeric(38,10)`, `numeric` sem precisão, `bigint` no limite de `i64`, `int`, `float8`, `text` grande, `bytea`, `uuid`, `json`/`jsonb`, `bool`, `timestamptz` com offset, `date`, `interval`, `NULL` em cada um, e array.
- **Bind:** escalar, `NULL`, lista via `= ANY($1)` (0, 1, 1000, 10.000 elementos).
- **Limites:** `max_rows` 0/1/1000; `truncated` correto na fronteira exata.
- **Cancelamento:** antes de começar, no meio, e depois de terminar (idempotente).
- **Read-only:** `SHOW transaction_read_only` = `on`.
- **Sanitizador:** erro contendo `postgres://user:senha@host:5432/db` → nada sobrevive.
- Todos com feature `integration`; `cargo test` sem Docker continua passando.

---

## Passo 3.5 — Store, keyring, esqueleto de IPC

```
src-tauri/src/store/mod.rs
src-tauri/src/store/migrations/0001_init.sql   # schema de D4
src-tauri/src/store/connections.rs
src-tauri/src/store/queries.rs
src-tauri/src/secrets.rs                        # crate `keyring`, service='queryboard'
src-tauri/src/ipc/mod.rs
src-tauri/src/ipc/connections.rs
src-tauri/src/ipc/query.rs
src-tauri/src/model/mod.rs                      # structs serde canônicos (JSON no SQLite == YAML no export)
src-tauri/build.rs ou bin de geração ts-rs
```

SQLite em `$APPDATA/queryboard/app.db` com `sqlx` (mesmo crate do driver Postgres, evita segunda biblioteca) e `sqlx::migrate!`. `secrets.rs` sobre o crate `keyring`, `service="queryboard"`, `account=connection.id`. Comandos IPC: `connection_list/create/update/delete/test`, `query_run`, `query_cancel`. **Nenhum comando devolve senha, em nenhuma forma.** `ts-rs` gerando `src/ipc/types.ts`.

**Pronto quando:** migrations aplicam em banco novo e existente; senha vai para o keyring e o `app.db` não contém nenhum byte dela (teste que grepa o arquivo); `types.ts` gerado bate com o commitado no CI; grep por `password` em struct de retorno de IPC não encontra nada.

**Testes:** CRUD de connection contra SQLite temporário; round-trip serde JSON↔struct; keyring com backend mock em CI (o real não funciona em runner headless — feature de mock ou `#[ignore]`); teste que serializa um `ConnectionConfig` com senha e afirma que a saída não a contém.

---

## Passo 4 — UI mínima + cancelamento ponta a ponta

```
src/ipc/types.ts                          # gerado por ts-rs
src/ipc/client.ts                         # wrapper tipado sobre invoke
src/features/connections/ConnectionList.tsx
src/features/connections/ConnectionForm.tsx
src/features/connections/ReadOnlyWarning.tsx   # aviso do §2.3
src/features/queries/QueryEditor.tsx           # CodeMirror 6, tudo bundlado
src/features/queries/ParamsPanel.tsx
src/features/queries/RunBar.tsx                # executar, cancelar, tempo, contagem, truncado
src/components/ResultGrid.tsx                  # TanStack Table v8 + react-virtual
src/components/ErrorPanel.tsx
src/App.tsx
src/features/**/__tests__/*.test.tsx
```

1. Cadastro de connection com o **banner de aviso** de credencial `SELECT`-only (§2.3), reforçado pela introspecção de privilégios do passo 3.
2. Editor de SQL com CodeMirror 6 e `@codemirror/lang-sql`, **tudo local** — a CSP proíbe CDN. Erro do guard inline, com o motivo específico (não "SQL inválida"), porque é a mensagem que o usuário mais vai ver.
3. Painel de parâmetros gerado a partir da extração do `params.rs`.
4. Grade com TanStack Table v8 + virtualização. Decimais renderizados como string, monoespaçados, à direita (D6). Banner de truncamento quando `truncated`.
5. **Botão cancelar funcionando de verdade**: `query_cancel` → `CancellationToken` → `pg_cancel_backend`. Fecha o risco R5 antes do flow existir.

**Pronto quando** (roteiro manual, contra um Postgres em container)
1. Cadastrar connection, testar, ver o banner de aviso.
2. Salvar `SELECT * FROM tb WHERE id = :id`; o painel de parâmetros mostra `id` sozinho.
3. Executar; grade com nomes de coluna em minúsculo, tempo e contagem.
4. Executar com `max_rows` estourado; banner de truncamento aparece.
5. Executar query lenta e cancelar; UI volta a `cancelled` em < 1s e o backend some de `pg_stat_activity`.
6. Colar `DROP TABLE x`; a UI mostra a rejeição do guard **com o motivo**, e nada chega ao banco.
7. Colar SQL com erro de sintaxe; o erro do Postgres aparece, sem host, usuário ou senha.
8. `pnpm tsc --noEmit`, `pnpm test`, `cargo test`, `cargo clippy -D warnings` limpos.

**Testes:** vitest + Testing Library com `client.ts` mockado — validação do formulário, render da grade com 1000 linhas (virtualização ativa), banner de truncamento, estado de cancelamento, decimal como string sem `Number()`, painel de erro sem campos ausentes. `pnpm tsc --noEmit` conta como teste (§8 proíbe `any`).

---

## Passo 4.5 — Checkpoint de empacotamento

`pnpm tauri build` no CI para os três SOs; instalar de fato o artefato no SO alvo; medir o tamanho do bundle contra a estimativa do §3 (~10 MB Rota A / ~50 MB Rota B); registrar o que falta de assinatura/notarização em `docs/adr/0005-empacotamento.md`. **Se a Rota B ganhou, este passo é obrigatório aqui e não depois** — PyInstaller + antivírus só aparece na máquina do usuário.

---

# Questões em aberto (com recomendação)

| # | Ambiguidade no CLAUDE.MD | Recomendação |
|---|---|---|
| Q1 | Ponto e vírgula final é aceito? O §2 diz "nada de `;` separando comandos", que não é a mesma coisa que `;` terminal | Aceitar **exatamente um** `;` terminal seguido só de whitespace, e removê-lo antes de enviar (Oracle thin rejeita terminador) |
| Q2 | `TABLE(...)` / table functions e chamadas a funções armazenadas em `SELECT` são leitura ou execução? | Rejeitar em v1 (§2: "na dúvida, bloqueie"). Se necessário, allowlist por connection num item futuro — decisão consciente, não silenciosa |
| Q3 | Representação de `NUMBER` de alta precisão no CEL. O §5 escreve `row.offer_status == 5` (int), o §6.5 escreve `row.preco != origin.preco` (decimal exato) | D6: escala 0 cabendo em `i64` → int CEL; qualquer outro numérico → string decimal canônica. Documentar na UI junto do aviso de minúsculas |
| Q4 | Veredito `unmatched` (§5) exige saber "qual campo" a regra cobre, mas uma expressão CEL é opaca | Acrescentar `subject_column` opcional no modelo de Rule (auto-preenchido pelo builder, manual no modo expressão). **Sem isso o `unmatched` é indeterminável** |
| Q5 | "Histórico de execuções" (item 15) vs "sem cache de resultados" (§11) | Guardar **só metadados** (timings, estados, motivos de skip, vereditos, contagens). Nunca linhas. Registrar no ADR |
| Q6 | `steps.<id>` de um passo `skipped`/`empty` — existe no contexto CEL? | Existe, com `rowcount = 0` e um campo `state`; referenciar `steps.x.rows[0]` de passo pulado vira `rule_error` isolado, não derruba o painel |
| Q7 | `correlate_on` entre bancos com tipos diferentes (`NUMBER` vs `VARCHAR`) — o §9 exige o teste mas não define a semântica | Comparar pela string canônica após trim; aviso visível de "chaves de tipos diferentes correlacionadas", nunca falhar em silêncio |
| Q8 | `max_rows` com lotes no Oracle: por lote ou total? | **Total**, com parada antecipada; o painel mostra "3 lotes, truncado em 1000" |
| Q9 | **`ORDER BY` + lotes de 1000 quebra a ordenação global** (§6.4 não trata) | v1: detectar `ORDER BY` raiz no AST do guard e avisar no detalhe da execução. v2: reordenar no cliente quando as chaves são colunas simples |
| Q10 | Workspace Cargo (`src-tauri` fino + crates de core) vs estrutura literal do §4 | Ficar com o §4 literal, tudo pendurado em `lib.rs` e só `ipc/` conhecendo `tauri::`. Revisitar se o tempo de compilação incomodar |
| Q11 | `params.<nome>` numa execução de query única, fora de flow | Mesma forma de contexto do flow, com o mapa vindo do painel de parâmetros. Assim uma regra escrita no item 7a continua válida no item 6 |

---

## Fontes consultadas

- [oracledb (crates.io)](https://crates.io/crates/oracledb) · [oracledb 0.9.1 (docs.rs — build nightly)](https://docs.rs/crate/oracledb/latest) · [MuhDur/rust-oracledb](https://github.com/MuhDur/rust-oracledb)
- [oracle 0.6.3 / kubo rust-oracle (ODPI-C)](https://docs.rs/crate/oracle/latest) · [GitHub](https://github.com/kubo/rust-oracle)
- [sqlparser 0.62.0](https://docs.rs/crate/sqlparser/latest) · [dialetos, inclui OracleDialect](https://docs.rs/sqlparser/latest/sqlparser/dialect/index.html) · [apache/datafusion-sqlparser-rs](https://github.com/apache/datafusion-sqlparser-rs)
- [cel 0.14.1](https://crates.io/crates/cel) · [cel-rust/cel-rust](https://github.com/cel-rust/cel-rust) · [FOSDEM 2026 — CEL in Rust](https://fosdem.org/2026/schedule/event/DBGZAU-rust-cel/)
- [sqlx 0.9.0](https://crates.io/crates/sqlx)
- [serde-yaml-ng](https://github.com/acatton/serde-yaml-ng) · [discussão da descontinuação do serde_yaml](https://users.rust-lang.org/t/serde-yaml-deprecation-alternatives/108868)
- [ts-rs 12.0.1](https://docs.rs/crate/ts-rs/latest) · [tauri-specta (estável ainda v1)](https://docs.rs/crate/tauri-specta/latest)
- [Tauri v2 — sidecar / externalBin](https://v2.tauri.app/develop/sidecar/) · [create-tauri-app](https://github.com/tauri-apps/create-tauri-app)
- [python-oracledb thin mode](https://python-oracledb.readthedocs.io/en/latest/user_guide/installation.html)
- [tauri-plugin-keyring (não recomendado — expõe senha ao JS)](https://github.com/charlesportwoodii/tauri-plugin-keyring)
