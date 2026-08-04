# CLAUDE.md

Contexto e regras de trabalho para este repositório. Leia antes de gerar ou alterar código.

---

## 1. O que é o projeto

**Nome de trabalho:** `queryboard` (renomear se preferir)

Um **app desktop leve, multi-banco e somente leitura** para executar **vários SELECTs de uma vez**, ver todos os resultados em uma única tela e **interpretar automaticamente** esses resultados através de regras que o próprio usuário escreve.

### A dor que resolve

Hoje a investigação de um caso exige abrir o cliente SQL e rodar manualmente uma sequência de selects, um por vez, copiando IDs de um resultado para o próximo e lembrando de cor o que cada código de status significa. Exemplo de uma investigação típica:

1. Consulta da **oferta** → devolve `offer_status = 5`; o usuário precisa lembrar que 5 é "decorrendo"
2. Consulta do **produto** → usa o `product_id` que veio do passo 1
3. Consulta do **envio do produto para a loja**
4. Consulta do **relacionamento oferta × loja × produto**

Isso leva muito tempo, é repetitivo e depende de conhecimento tácito. A ferramenta transforma essa sequência em: **informar um parâmetro (ex.: `offer_id`) → apertar um botão → ver os resultados lado a lado, já anotados com o diagnóstico.**

### Os quatro conceitos centrais

| Conceito | O que é |
|---|---|
| **Connection** | Uma conexão a um banco (Oracle, PostgreSQL, MySQL, ...). Sempre somente leitura. |
| **Query** | Um SELECT salvo, com parâmetros nomeados (`:offer_id`), vinculado a uma connection. |
| **Rule** | Uma condição sobre o resultado de uma query que produz um veredito legível ("Oferta decorrendo"). Escrita pelo usuário. |
| **Flow** | Um grafo de queries que compartilham parâmetros, propagam valores entre si e são executadas em paralelo onde possível. |

Um flow pode conter queries apontando para **bancos diferentes** — isso é requisito, não extra.

---

## 2. Regra inviolável: somente leitura

Esta é a regra mais importante do projeto. **A aplicação nunca escreve no banco.**

Proibido em qualquer circunstância: `INSERT`, `UPDATE`, `DELETE`, `MERGE`, `TRUNCATE`, `DROP`, `CREATE`, `ALTER`, `GRANT`, `REVOKE`, `CALL`/`EXEC` de procedures, `COMMIT`, blocos anônimos PL/SQL, `DO $$`, e qualquer DDL/DML.

A garantia é feita em **camadas** — nenhuma sozinha é suficiente:

1. **Parser** — a SQL é parseada e só é aceita se o statement raiz for `SELECT` ou `WITH ... SELECT`. Rejeitar múltiplos statements (nada de `;` separando comandos). Rejeitar CTE que contenha DML (`INSERT ... RETURNING` dentro de `WITH`).
2. **Sessão do banco** — abrir a sessão em modo leitura sempre que o driver permitir (`SET TRANSACTION READ ONLY` no Postgres/Oracle; `SET SESSION TRANSACTION READ ONLY` no MySQL). Fazer `rollback` ao final, nunca `commit`.
3. **Permissão do usuário do banco** — documentar no README que a credencial usada deve ser de um usuário `SELECT`-only. A aplicação **avisa** isso na UI ao cadastrar uma connection.
4. **Sem string interpolation** — todo parâmetro entra como bind variable. Nunca concatenar valor de usuário em SQL.
5. **Rules e flows não executam SQL própria.** Uma regra avalia dados já buscados; ela nunca gera, monta ou dispara SQL por conta própria. Um flow só executa queries que já existem cadastradas e validadas.

Se estiver em dúvida se algo é seguro, **bloqueie**. Falso positivo é aceitável; falso negativo não é.

---

## 3. Stack

App desktop com **Tauri v2** (binário pequeno, sem Electron, usa o webview do SO).

- **Shell/core:** Rust + Tauri v2
- **Frontend:** React + Vite + TypeScript, TanStack Table para as grades
- **Estado local:** SQLite (`$APPDATA/queryboard/app.db`) para connections, queries, rules e flows
- **Regras:** CEL (Common Expression Language) — linguagem de expressão sandboxed, sem loops, sem I/O, sem chamada de função arbitrária. Ver seção 5.
- **Testes:** `cargo test` no core, `vitest` no front

### Camada de banco — decisão do Oracle (fechada)

Ver `docs/adr/0001-camada-oracle.md` (descarte da Rota A, `oracledb` exige nightly) e
`docs/adr/0006-camada-oracle-rota-b-odpic.md` (Rota B revisada para o crate `oracle`/ODPI-C
depois do follow-up K1–K9 contra Oracle real, substituindo a recomendação original de
sidecar Python da 0001).

O ponto sensível era o Oracle. Duas rotas foram avaliadas:

| | **Rota A — Rust puro** | **Rota B — crate OCI/Instant Client** |
|---|---|---|
| Oracle | crate `oracledb`/`oraclemcp-driver-cx` (thin, sem Instant Client) | crate `oracle` (ODPI-C) |
| Postgres / MySQL | `sqlx` | `sqlx` |
| Bundle | ~10 MB, binário único | binário único + Oracle Instant Client instalado na máquina |
| Risco | crate Oracle jovem, menos rodado em produção | crate maduro (2.3M+ downloads), mas exige lib nativa da Oracle em runtime |

Spike real (roadmap item 1) rodado contra um Oracle Database Free local
(`gvenzl/oracle-free:23-slim`, via `docker compose --profile oracle`):

- **Rota A descartada**: o crate `oracledb` está no meio de uma
  transferência de nome/propriedade para a Oracle Corp (README do projeto:
  "the original `oracledb`... names are being handed to Oracle"), o código
  ativo continua como `oraclemcp-driver-cx` — crate com ~7 semanas de
  existência, poucos downloads, README com alegações de benchmark
  infladas para algo tão novo. Mais grave: **o feature-set `default` da
  dependência `asupersync` exige Rust nightly pra compilar**
  (`#![feature(try_trait_v2)]`), sem opção de desligar isso do lado do
  consumidor — bloqueio duro, não avaliação de risco.
- **Rota B confirmada**: crate `oracle` (ODPI-C) validado de ponta a ponta
  contra o Oracle Database Free real — conexão via EasyConnect, `NUMBER`
  de alta precisão (decodificado como string, nunca `f64`/`rust_decimal`),
  `CLOB`, `DATE` (inclui hora), `TIMESTAMP WITH TIME ZONE`,
  `SET TRANSACTION READ ONLY` bloqueando escrita (`ORA-01456`), bind
  nomeado e bind posicional com placeholder repetido (`:id + :id` com um
  valor só — confirma que a ordem de `bind_order` do `sql/params.rs` já
  bate com a ordem posicional que o driver ODPI-C espera, sem precisar de
  nome). Cancelamento via `Connection::break_execution()` chamado de outra
  thread sobre a mesma conexão (`Connection` é `Send + Sync`), sem
  precisar de conexão auxiliar como Postgres/MySQL. O crate carrega
  `libclntsh` (Oracle Instant Client) via `dlopen` em **runtime**, não em
  link-time — buildar o projeto não exige Instant Client instalado, só
  conectar de verdade exige.
- `tnsnames.ora`/wallet **não foi validado** (o ambiente onde isso foi
  implementado não tinha um `tnsnames.ora` real disponível) — só o
  caminho EasyConnect (`host:porta/serviço`) foi testado. Documentado como
  limitação conhecida em `README.md`.

Implementação em `db/oracle.rs`. Ver também `db/mysql.rs` (sem decisão em
aberto — `sqlx` cobre MySQL sem ressalvas).

**O código de banco fica atrás do trait `Driver`** (`connect`,
`execute_select`, `cancel`), sem vazar tipos específicos de driver para o
resto da aplicação — isso permitiu trocar de rota depois do spike sem
tocar em mais nada fora de `db/oracle.rs`.

---

## 4. Estrutura de diretórios

```
/
├── src-tauri/
│   └── src/
│       ├── main.rs
│       ├── ipc/               # comandos expostos ao frontend
│       ├── db/
│       │   ├── driver.rs      # trait Driver
│       │   ├── oracle.rs
│       │   ├── postgres.rs
│       │   └── mysql.rs
│       ├── sql/
│       │   ├── guard.rs       # ← validação read-only. Núcleo crítico.
│       │   └── params.rs      # extração e bind de :parametros
│       ├── rules/
│       │   ├── engine.rs      # avaliação CEL
│       │   ├── model.rs       # schema de Rule
│       │   └── verdict.rs
│       ├── flow/
│       │   ├── graph.rs       # DAG, ciclos, ordem topológica
│       │   └── runner.rs      # execução, gating, propagação
│       ├── store/             # SQLite local
│       └── secrets.rs         # keyring do SO
├── src/                       # frontend React
│   ├── components/
│   ├── features/              # flow/, connections/, queries/, rules/
│   └── ipc/                   # wrapper tipado dos comandos Tauri
└── CLAUDE.md
```

---

## 5. Rules — interpretação dos resultados

### O problema

`offer_status = 5` não diz nada sozinho. Significa "Decorrendo", e o usuário sabe disso de cabeça. A ferramenta deve permitir que ele **codifique esse conhecimento uma vez** e nunca mais precise lembrar.

### Modelo

Uma rule é um dado (não código), armazenada em SQLite e exportável como YAML/JSON para ser compartilhada com o time.

```yaml
id: oferta_decorrendo
query: consulta_oferta
scope: row                    # row | result
priority: 100
when: "row.offer_status == 5"
then:
  severity: info              # ok | info | warn | error
  title: "Oferta decorrendo"
  message: "Oferta {{row.offer_id}} em decurso desde {{row.start_date}}."
```

```yaml
id: produto_nao_enviado
query: consulta_envio_loja
scope: result                 # avalia o conjunto, não a linha
when: "result.rowcount == 0"
then:
  severity: warn
  title: "Produto não enviado para a loja"
  message: "Nenhum registro de envio encontrado para o produto {{params.product_id}}."
```

### Contexto disponível na expressão

- `row.<coluna>` — apenas em `scope: row`
- `result.rowcount`, `result.rows`, `result.columns` — em `scope: result`
- `params.<nome>` — os parâmetros de entrada do flow
- `steps.<query_id>.rowcount` — resultado de um passo anterior do flow (leitura, para regras cruzadas)
- `origin.<coluna>` — a linha correspondente no passo de origem, quando o passo declara `correlate_on`. É o que permite regras de divergência entre tabelas (ver seção 6, item 5).

Nomes de coluna são normalizados para minúsculo (o Oracle devolve maiúsculo por padrão). Documentar isso na UI do editor de regras.

### Semântica de avaliação

- **Regras nunca escondem o dado.** O veredito é uma anotação acima da grade, jamais um substituto dela. Se uma regra estiver errada durante um incidente, o usuário ainda precisa enxergar a linha crua.
- **Todas as regras que casam produzem veredito.** Não há "primeira que casa vence". O badge do painel mostra a maior severidade; a lista mostra todos, ordenados por `priority` desc e depois por `id` (determinístico).
- **Gap explícito.** Se um campo tem regras cadastradas mas nenhuma casou para aquele valor, emitir um veredito automático `unmatched`: "offer_status = 99 — sem regra cadastrada". Buraco de conhecimento silencioso é pior que buraco visível.
- **Pureza.** A avaliação é função pura sobre dados já buscados. Sem I/O, sem rede, sem acesso a arquivo, sem relógio. Determinística.
- **Falha isolada.** Erro numa regra (coluna inexistente, tipo incompatível) marca aquela regra como `rule_error` e segue avaliando as demais. Nunca derruba o painel.
- **Limites.** Timeout por regra e teto de linhas avaliadas em `scope: row` (padrão: 1000). Regras rodam sobre o resultado já truncado por `max_rows`.

### Editor

Duas formas de escrever a mesma regra, na mesma tela:

- **Builder** (padrão): coluna → operador → valor → mensagem. Gera CEL por baixo.
- **Expressão** (avançado): CEL direto, com validação em tempo real e preview contra o último resultado obtido.

O preview contra dados reais é o que torna isso usável — o usuário vê imediatamente se a regra casou.

---

## 6. Flow — encadeamento de queries

Flow é o grafo de execução. Ele resolve o "copiar ID de um resultado e colar no próximo select".

### O que um flow define

1. **Parâmetros de entrada** — o que o usuário digita uma vez (ex.: `offer_id`). Parâmetros de mesmo nome em queries diferentes compartilham o valor.
2. **Passos (steps)** — cada passo é uma query.
3. **Propagação** — um passo alimenta seus parâmetros com colunas do resultado de um passo anterior. A cardinalidade do passo anterior determina o modo:

   **`collect` (padrão)** — a coluna inteira vira uma lista, e o passo seguinte roda **uma vez** com `IN`. É o caso mais comum e o mais eficiente.

   ```yaml
   - step: consulta_processo
     depends_on: [consulta_plu_loja]
     bind:
       idprocessos:                          # parâmetro do tipo list
         from: consulta_plu_loja
         column: idprocesso
         mode: collect
         distinct: true
         drop_nulls: true
     correlate_on: idprocesso                # ver "Correlação" abaixo
     on_empty: skip                          # skip | fail
   ```

   Com a SQL do passo escrita normalmente:
   ```sql
   SELECT * FROM tb_processo WHERE idprocesso IN (:idprocessos)
   ```

   **`first`** — usa apenas a primeira linha. Só quando o passo anterior é garantidamente 1:1.

   **`fan_out`** — executa o passo **uma vez por linha** do anterior, gerando um painel por execução. Usar só quando a query de destino não aceita `IN` (ex.: função que recebe um valor escalar) ou quando você precisa ver os resultados separados. Custa N round-trips; teto padrão de 50 execuções, configurável.

4. **Parâmetros de lista** — declarados como `list<number>` / `list<string>`. A expansão para o dialeto é responsabilidade do `params`, **sempre como bind variables, nunca concatenação**:

   | Banco | Expansão |
   |---|---|
   | PostgreSQL | `= ANY($1)` com array nativo |
   | MySQL | expande para `%s, %s, ...` |
   | Oracle | expande para `:p0, :p1, ...` |

   **Oracle tem limite de 1000 elementos em `IN`.** Acima disso, o runner divide em lotes de 1000, executa os lotes e concatena os resultados antes de entregar ao painel — de forma transparente para o usuário, mas registrada no detalhe da execução ("3 lotes"). Lista vazia nunca vira `IN ()` (SQL inválido): o passo é marcado `empty` conforme `on_empty`.

   Teto de tamanho de lista configurável (padrão 10.000) para não montar uma consulta absurda por engano.

5. **Correlação** — `correlate_on` declara a chave que liga as linhas do passo atual às do passo anterior. Isso habilita duas coisas:
   - Na UI, clicar numa linha do painel de origem **destaca as linhas correspondentes** nos painéis derivados.
   - Nas regras, o contexto `origin` fica disponível em `scope: row`, permitindo comparar tabelas:
     ```yaml
     id: preco_divergente
     query: consulta_processo
     scope: row
     when: "row.preco != origin.preco"
     then:
       severity: error
       title: "Preço divergente"
       message: "PLU/Loja tem {{origin.preco}}, processo {{row.idprocesso}} tem {{row.preco}}."
     ```
     Este é o padrão de uso mais valioso da ferramenta: **cruzar o mesmo dado em duas tabelas e apontar a diferença sozinho.** Sem `correlate_on`, `origin` não existe e regras que o referenciam falham na validação, ao salvar.

   Linhas do passo derivado sem correspondência na origem (e vice-versa) são marcadas como `orphan` no painel — órfão silencioso esconde exatamente o tipo de bug que se está caçando.

6. **Gating condicional** — um passo só roda se a condição for verdadeira:
   ```yaml
   - step: consulta_envio_loja
     depends_on: [consulta_oferta]
     run_if: "steps.consulta_oferta.rows[0].offer_status == 5"
   ```

### Execução

- Ordem topológica; passos independentes rodam **em paralelo**.
- Estados por passo: `pending | running | ok | empty | skipped | error | timeout | cancelled`.
- **Passo pulado mostra o motivo**: "pulado — `run_if` falso (offer_status = 3)". Um passo cinza sem explicação é inútil na hora do incidente.
- Erro ou timeout em um passo **não derruba** os passos independentes dele; derruba apenas os descendentes, que ficam `skipped` com a causa apontando para o passo que falhou.
- Ciclos são detectados **ao salvar**, não em execução.
- Todo flow é cancelável; cancelar aborta os cursores abertos.
- Limites por connection (configuráveis): `max_rows = 1000`, `timeout = 30s`. `max_rows` é aplicado via fetch limitado no cursor, nunca injetando `LIMIT` na SQL do usuário.

---

## 7. Credenciais e segurança do app

- Senhas **nunca** em texto plano, nunca versionadas. Keyring do SO (`tauri-plugin-keyring` / `keyring` crate).
- Senha nunca volta pelo IPC, nem em log, nem em mensagem de erro.
- CSP estrita no Tauri; **sem carregar conteúdo remoto** no webview. O app é offline exceto pelas conexões de banco.
- Allowlist mínima de plugins Tauri. Não habilitar `shell`, `fs` amplo ou `http` sem necessidade justificada.
- Erros de banco são repassados ao usuário (ele precisa deles para depurar SQL), **mas passam por um sanitizador que remove host, usuário e senha**.
- Export de rules/flows **não inclui credenciais** — apenas o identificador lógico da connection, que o destinatário mapeia para a dele.

---

## 8. Convenções de código

- **Rust:** `clippy` sem warnings; erros com `thiserror`; nada de `unwrap()` fora de teste; async com `tokio`.
- **TypeScript:** `strict: true`; sem `any`; componentes funcionais com hooks.
- **IPC:** todo comando Tauri tem tipo espelhado no front, gerado ou mantido em `src/ipc/types.ts`. Divergência entre Rust e TS é bug.
- **Sem ORM para a SQL do usuário.** A SQL vai crua (validada) para o driver. ORM, se houver, só para o SQLite interno.
- Commits: Conventional Commits (`feat:`, `fix:`, `refactor:`, `test:`).

---

## 9. Testes

- `src-tauri/src/sql/guard.rs` é o arquivo mais crítico do repositório. **Toda alteração nele exige novos casos de teste**, incluindo tentativas de bypass: comentários (`SELECT 1; -- \n DROP`), homóglifos unicode, `WITH` contendo DML, statements empilhados, `/*+ hint */`, string literal com `;`.
- O engine de regras precisa de testes de: expressão inválida, coluna ausente, `null`, tipo incompatível, resultado vazio, e ordenação determinística com múltiplas regras casando.
- O runner de flow precisa de testes de: ciclo, `on_empty`, gating falso, falha de dependência em cascata, cancelamento no meio.
- Parâmetros de lista têm casos obrigatórios: lista vazia, 1 elemento, exatamente 1000, 1001 (lote no Oracle), 10.001 (estouro do teto), valores duplicados com `distinct`, nulos com e sem `drop_nulls`, e tipos mistos na coluna.
- Correlação: chave ausente em um dos lados, chave duplicada nos dois lados (produto cartesiano — deve ser detectado e avisado, não expandido silenciosamente), tipos diferentes nos dois lados (`NUMBER` vs `VARCHAR`).
- Testes de driver usam containers efêmeros (Postgres e MySQL). Oracle é mockado no CI e validado manualmente — não adicionar dependência de Oracle no pipeline.
- Nenhum teste toca banco de produção.

---

## 10. Comandos

```bash
pnpm install
pnpm tauri dev            # desenvolvimento
pnpm tauri build          # build do app

cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
pnpm test && pnpm tsc --noEmit
```

---

## 11. Escopo — o que NÃO fazer

Não implemente, mesmo que pareça útil:

- Qualquer caminho de escrita no banco (ver seção 2)
- Regras ou flows que **gerem** SQL. Regra interpreta resultado; ela não constrói consulta.
- Regras com efeito colateral: enviar e-mail, chamar webhook, escrever arquivo, abrir chamado. Regra produz veredito na tela e nada mais.
- Loops, recursão ou funções definidas pelo usuário na linguagem de regras. Se o usuário precisar disso, o desenho está errado.
- Editor de schema, browser de tabelas com "edit cell", migrations
- Autenticação multiusuário / RBAC / servidor compartilhado — **é ferramenta local, mono-usuário**
- Cache de resultados entre execuções (o usuário quer o dado de agora)
- Geração de SQL por IA dentro do produto
- Gráficos e dashboards visuais — o output é **tabela + veredito**

Se uma tarefa parecer exigir algo desta lista, pare e pergunte antes de codar.

---

## 12. Roadmap (ordem sugerida)

1. ~~**Spike Oracle**~~ — feito. Rota B (crate `oracle`, ODPI-C) confirmada — ver "Camada de banco — decisão do Oracle" acima.
2. `guard.rs` + suíte de testes de bypass
3. Driver Postgres + execução de uma query única com parâmetros
4. UI mínima: cadastro de connection, editor de SQL, uma grade de resultado
5. ~~Driver Oracle~~ — feito (`db/oracle.rs`, Rota B/ODPI-C).
6. Flow: N queries, parâmetros compartilhados, execução paralela, painéis
7. Rules: engine CEL, `scope: row` e `scope: result`, badges no painel
8. Propagação `first` + `run_if`
9. Parâmetros de lista e propagação `collect` (com lotes no Oracle) — é o que destrava o caso PLU+LOJA → processo
10. `correlate_on`, contexto `origin` e regras de divergência entre tabelas
11. Editor visual de regras com preview contra o último resultado
12. Modo `fan_out`
13. ~~Driver MySQL~~ — feito (`db/mysql.rs`, `sqlx`).
14. Export/import de rules e flows em YAML
15. Export CSV/JSON do painel; histórico de execuções