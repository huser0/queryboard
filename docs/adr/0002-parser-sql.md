# ADR 0002: Parser SQL para o guard read-only

- Status: aceita
- Data: 2026-07-31

## Contexto

`sql::guard` é o arquivo mais crítico do repositório (CLAUDE.md §9): precisa aceitar
`SELECT`/`WITH ... SELECT` legítimos nos três dialetos (Oracle, PostgreSQL, MySQL) e rejeitar
qualquer statement empilhado, DML escondido dentro de CTE, comentário escondendo comando,
hint, homóglifo unicode ou função com efeito colateral — com falso positivo aceitável e falso
negativo inaceitável (CLAUDE.md §2).

## Decisão

**`sqlparser` 0.62.0** (crate `sqlparser`, `apache/datafusion-sqlparser-rs`), com a feature
`visitor` ligada, usado em 3 camadas:

1. Pré-checagem textual (`sql::lexical`) — bytes de controle e caracteres invisíveis/bidi
   (Trojan Source), antes de qualquer tokenização, em qualquer posição do texto (inclusive
   dentro de literais e comentários).
2. Tokenização explícita (`Tokenizer::tokenize_with_location`) para exigir ASCII fora de
   literais/identificadores entre aspas/comentários, e para contar separadores `;` — antes
   de sequer tentar fazer parse.
3. Parse (`Parser::parse_statements`, com `OracleDialect`/`PostgreSqlDialect`/`MySqlDialect`
   e `with_recursion_limit(50)`) + walk via `Visitor` (não `match` manual) sobre o AST
   completo, rejeitando `SetExpr::{Insert,Update,Delete,Merge}` (inclusive dentro de CTE),
   `Query.locks` não vazio (`FOR UPDATE`/`FOR SHARE`), `Select.into` (`SELECT ... INTO`),
   `TableFactor::{Function,TableFunction}`, e chamadas a funções na lista de proibições por
   dialeto (`sql::denylist`).

O texto enviado ao driver é sempre o **original do usuário** (validado, com no máximo um `;`
terminal removido) — nunca uma reserialização do AST, para preservar hints (`/*+ ... */`) e
formatação byte a byte.

## Evidência

- v0.62.0, publicado em 07/mai/2026, governança Apache, ordem de 10M+ downloads recentes —
  maduro e mantido.
- Tem `OracleDialect` de primeira classe, junto de `PostgreSqlDialect` e `MySqlDialect` —
  os três dialetos do projeto têm parser dedicado, não um "genérico" aproximado.
- Confirmado por leitura direta do código-fonte (não por documentação):
  - `SetExpr` tem variantes `Insert(Statement)`, `Update(Statement)`, `Delete(Statement)`,
    `Merge(Statement)` — mais do que o previsto originalmente no PLAN.md (que citava só
    Insert/Update); todas as quatro são rejeitadas.
  - `Cte { query: Box<Query>, .. }` deriva `Visit`, então uma CTE com um desses `SetExpr`
    dentro é capturada pelo mesmo `pre_visit_query` que checa o corpo do `Query` de nível
    raiz — sem precisar de um caminho de código separado para CTE.
  - `:nome` tem suporte de primeira classe na gramática (`Expr::Value(Value::Placeholder)`),
    inclusive com a mesma exigência de adjacência sem espaço que o `sql::params` usa para
    extrair ocorrências — validado por teste (`accepts_named_bind_parameter`).
  - `TokenWithSpan.span` usa `Location { line, column }` (linha/coluna, contagem por `char`),
    não offset de byte — por isso `sql::location_to_byte_offset` existe: reconstrói o offset
    de byte replicando exatamente o algoritmo de `State::next()` do tokenizer (linha 1,
    coluna 1, coluna reinicia em `\n`), usado tanto para cortar o `;` terminal quanto para a
    reescrita de parâmetros em `sql::params`.
- 77 testes em `sql::{guard,params,lexical,denylist}` cobrindo a suíte de bypass exigida pelo
  CLAUDE.md §9 (statements empilhados, DML em CTE, comentários escondendo comando, hints
  preservados, homóglifo cirílico, caracteres invisíveis/bidi em qualquer posição, funções
  proibidas por dialeto, recursão limitada sem stack overflow) e os falsos positivos que têm
  que continuar funcionando (`WITH RECURSIVE`, subquery em `FROM`/`IN`/`EXISTS`/`SELECT`,
  identificador entre aspas com espaço, cast `::text` do Postgres não confundido com bind).

## Alternativas descartadas

- **Tokenizer/parser próprio**: custo alto exatamente no arquivo onde não se pode errar, sem
  o benefício de uma base de usuários grande já testando o parser.
- **Regex**: não distingue `;` dentro de um literal de string de um `;` separador de
  statement — é a categoria de bug mais perigosa possível aqui (falso negativo).
- **`sqlparser` + reserialização do AST**: perderia hints Oracle (`/*+ ... */`) e introduziria
  diferenças de formatação entre o que o usuário escreveu e o que roda no banco — descartado
  por design (ver CLAUDE.md §2.4, "sem string interpolation" e a decisão de nunca reserializar
  registrada em `sql/guard.rs`).

## Consequências

- `sqlparser` cobre um subconjunto do PL/SQL/T-SQL/dialetos exóticos e vai rejeitar SQL
  Oracle válida mas incomum — aceitável pelo princípio "na dúvida, bloqueie" (CLAUDE.md §2).
- Toda checagem estrutural nova (nova cláusula proibida, por exemplo) deve ser adicionada ao
  `Visitor` (`GuardVisitor` em `sql/guard.rs`), nunca como `match` solto em outro lugar —
  é o que garante que uma variante nova de AST numa versão futura do `sqlparser` não abra um
  buraco silencioso.
- `sql::params` depende de `sql::location_to_byte_offset`, que por sua vez depende do
  algoritmo exato de contagem de linha/coluna do tokenizer do `sqlparser`. Se uma versão
  futura do crate mudar essa contagem (improvável, é parte estável da API pública via
  `Location`), os testes de `params` pegam a divergência.
