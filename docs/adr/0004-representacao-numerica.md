# ADR 0004: `CellValue::Decimal` como string canônica — nunca `f64`

- Status: aceita
- Data: 2026-07-31

## Contexto

`NUMBER(38,10)` do Oracle e `NUMERIC` do Postgres não cabem em `f64` sem perda de precisão, e
JSON (o formato do IPC Tauri) converte número para `double` silenciosamente. Sem uma decisão
explícita sobre isso, a regra mais valiosa da ferramenta — `row.preco != origin.preco`
(CLAUDE.md §6.5, comparação de divergência entre tabelas) — daria falso positivo por erro de
ponto flutuante em produção, exatamente no tipo de caso que a ferramenta existe para pegar.

## Decisão

`CellValue::Decimal(String)` carrega uma string decimal canônica (sinal só quando negativo,
escala preservada exatamente como veio do banco, sem passar por nenhum tipo intermediário de
precisão limitada). Isso vale em toda a cadeia:

1. **No driver** (`db::postgres::decode_pg_numeric`, ADR 0003): decodifica o formato binário
   do protocolo direto, sem usar `rust_decimal` (que satura por volta de 28-29 dígitos
   significativos — menos que os 38 dígitos de um `NUMBER(38,x)`/`NUMERIC(38,x)`).
2. **No IPC** (ainda não implementado — chega no item 3.5/4): o tipo TS gerado deve ser
   `string`, nunca `number`. O front nunca chama `Number()` nela.
3. **No CEL** (roadmap item 7a, ainda não implementado): `NUMBER` com escala 0 que cabe em
   `i64` vira `int` do CEL (faz `row.offer_status == 5` funcionar como o exemplo do CLAUDE.md
   §5 escreve); qualquer outro numérico vira string decimal normalizada. Como a normalização é
   a mesma dos dois lados, `row.preco != origin.preco` compara strings e é exata.

## Evidência

Teste de integração `numeric_with_precision_and_scale_roundtrips_exactly` (`tests/pg_types.rs`)
contra Postgres 16 real: `12345678901234567890.1234567890::numeric(38,10)` (30 dígitos antes
da vírgula) roundtrip exato via `CellValue::Decimal`. Teste unitário
`decodes_value_exceeding_rust_decimal_precision` confirma que o decodificador cobre um valor
de 30 dígitos inteiros — acima do que `rust_decimal` conseguiria representar sem perda.

## Consequências

- Toda comparação numérica em regras precisa saber se está lidando com `int` (comparação
  numérica) ou `Decimal` (comparação de string). Isso precisa ser documentado na UI do editor
  de regras junto do aviso de nomes de coluna em minúsculo (CLAUDE.md §5) — pendente do item
  7a.
- `Bind::Decimal(String)` (o lado de escrita/bind, `db::driver::Bind`) faz o caminho inverso:
  recebe a string canônica e a converte para `rust_decimal::Decimal` antes de fazer bind no
  Postgres (`db::postgres::bind_all`). Isso **reintroduz** o teto de ~28-29 dígitos do
  `rust_decimal` do lado do bind — aceitável por ora porque parâmetros de valor alto precisão
  são raros (tipicamente IDs/status, não valores monetários de 38 dígitos), mas é uma
  assimetria consciente entre leitura (arbitrária) e escrita de parâmetro (limitada a
  ~28-29 dígitos), registrada aqui para não virar surpresa depois.
