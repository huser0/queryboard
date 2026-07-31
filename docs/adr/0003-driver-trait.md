# ADR 0003: Trait `Driver`/`Session` e driver Postgres

- Status: aceita
- Data: 2026-07-31

## Contexto

CLAUDE.md §3 exige que o código de banco fique atrás de um trait `Driver`, sem vazar tipos
de driver específicos para o resto da aplicação, para que a troca entre Rota A e Rota B do
Oracle (ADR 0001) custe um módulo, não uma refatoração. §6 exige fetch limitado no cursor
(nunca `LIMIT` injetado) e cancelamento de flow.

## Decisão

`db::driver::{Driver, Session}` — assinatura conforme `PLAN.md` D1: `Driver::connect` devolve
`Box<dyn Session>`; `Session::begin_read_only`/`execute_select`/`rollback_and_close`.
`execute_select` recebe `&ValidatedSql` (nunca `&str`), `&[Bind]`, `&Limits` e
`CancellationToken` (`tokio-util`). Nenhum tipo do `sqlx` (`PgRow`, `PgConnection`, ...)
aparece na assinatura do trait — só `CellValue`/`ResultSet`/`ColumnMeta`/`DbError`.

`db::postgres::PostgresDriver` implementa o trait sobre `sqlx` 0.9. Pontos que só se resolvem
com o crate real na mão, não só lendo documentação:

- **`sqlx::AssertSqlSafe`**: a partir da 0.9, `sqlx::query()` exige `impl SqlSafeStr`, que só é
  implementado nativamente para `&'static str` — qualquer string dinâmica precisa de
  `AssertSqlSafe(...)` explícito. Isso valida a arquitetura do projeto por fora: `ValidatedSql`
  é exatamente a prova de segurança que justifica o `AssertSqlSafe`, e é o único lugar do
  driver que usa esse wrapper.
- **Metadados de coluna vêm de `conn.prepare(sql).await?.columns()`**, separado do
  `execute_select`. Um resultado de zero linhas não devolve nenhuma `PgRow` da qual extrair
  colunas — só `prepare()` garante isso.
- **`NUMERIC` nunca passa por `rust_decimal`.** `rust_decimal::Decimal` satura por volta de
  28-29 dígitos significativos (mantissa de 96 bits) — insuficiente para `NUMERIC(38,x)`, que
  é exatamente o caso que a regra de divergência de preço (CLAUDE.md §6.5) precisa cobrir
  sem erro. `db::postgres::decode_pg_numeric` decodifica o formato binário do protocolo
  Postgres direto (2 bytes ndigits, weight i16, sign, dscale, seguidos de grupos base-10000),
  documentado em `backend/utils/adt/numeric.c` do próprio Postgres. Testado com um valor de
  30 dígitos inteiros — acima do limite do `rust_decimal` — contra um Postgres 16 real via
  testcontainers, roundtrip exato.
- **Cancelamento via conexão auxiliar + `pg_cancel_backend(pid)`.** `sqlx-postgres` rastreia
  `process_id`/`secret_key` internamente (necessários para o protocolo nativo `CancelRequest`)
  mas não expõe API pública para isso. A técnica padrão — abrir uma segunda conexão e chamar
  `pg_cancel_backend($1)` — funciona com qualquer cliente Postgres e foi testada de verdade:
  uma query de ~25M linhas (produto cartesiano de duas `generate_series` de 5000) cancelada
  em bem menos de 5s.
- **Resolução de segredo injetada via closure** (`PostgresDriver::new(resolve_secret: impl
  Fn(&SecretRef) -> Result<String, DbError>)`), não hardcoded no driver. `secrets.rs`
  (roadmap item 3.5, keyring do SO) só precisa fornecer essa closure na hora de construir o
  driver — o driver em si não sabe (nem precisa saber) onde a senha mora.
- **Tipos sem mapeamento dedicado** (array, range, tipo de domínio custom) degradam para um
  marcador de texto legível em vez de falhar a query inteira — testado com `ARRAY[1,2,3]`
  contra Postgres real.

## Evidência

23 testes de integração em `tests/pg_types.rs`, todos passando contra Postgres 16 real
(container efêmero via `testcontainers`, `podman` como backend Docker-compatível neste
ambiente): `NUMERIC(38,10)` exato, `NUMERIC` sem precisão declarada, `bigint` no limite de
`i64`, `int`, `float8`, texto de 100.000 caracteres, `bytea`, `uuid`, `json`/`jsonb`, `bool`,
`timestamptz` com offset, `date`, `interval`, `NULL` em cada um dos catorze tipos mapeados,
bind escalar por posição, bind `NULL`, `max_rows` truncando via fetch limitado (nunca
`LIMIT`), sessão `transaction_read_only = on` confirmada via `current_setting()`, cancelamento
antes e durante a execução, e mensagem de erro de sintaxe sem vazar senha/host.

12 testes unitários adicionais (`decode_pg_numeric`, sanitizador de erro, `ColumnMeta`,
`SecretRef` Debug redigido).

## Consequências

- Trocar para Oracle (Rota A ou B, ver ADR 0001) implica implementar `Driver`/`Session` de
  novo em `db/oracle.rs`, reaproveitando `Bind`, `Limits`, `CellValue`, `ResultSet`,
  `DbError` sem alteração. `decode_pg_numeric` é específico do formato de fio do Postgres —
  Oracle exige seu próprio decodificador de `NUMBER`, mas a mesma exigência ("nunca passa por
  tipo de precisão limitada") se aplica.
- `Bind::Null` hoje é sempre enviado como `Option::<String>::None` — funciona para a maioria
  dos casos mas pode falhar contra um parâmetro cujo tipo inferido pelo Postgres não aceita
  coerção implícita de texto. Corrigir exige os metadados de tipo declarado do parâmetro, que
  só existem a partir do item 3.5 (`Query.params_json`). Registrado como limitação conhecida,
  não como bug silencioso.
- O container de teste é compartilhado entre os 23 testes (`tokio::sync::OnceCell` estático)
  para não pagar o custo de subir Postgres 23 vezes — por isso os testes de integração rodam
  com `--test-threads=1` (mesmo estado de banco entre testes que rodam em paralelo seria uma
  fonte de flakiness).
