# Roteiro de validação manual — driver Oracle

O driver Oracle (`src-tauri/src/db/oracle.rs`) **não** tem testes de
integração automatizados via testcontainers, ao contrário de Postgres
(`tests/pg_types.rs`) e MySQL (`tests/mysql_types.rs`) — por instrução
explícita do `CLAUDE.md` §9: "testes de driver usam containers efêmeros
(Postgres e MySQL). Oracle é mockado no CI e validado manualmente — não
adicionar dependência de Oracle no pipeline."

Rode este roteiro à mão sempre que `db/oracle.rs` mudar. Cada item foi
validado pelo menos uma vez (durante o spike que decidiu a Rota B — ver
`CLAUDE.md`, "Camada de banco — decisão do Oracle") contra um Oracle
Database Free real local; nada aqui é hipotético.

## Preparar o ambiente

```bash
docker compose --profile oracle up -d --wait
```

Sobe `gvenzl/oracle-free:23-slim` em `localhost:51521`, serviço
`FREEPDB1`, populado com `dev/seed-oracle.sql` (domínio RH/folha —
`payroll_id` 9001-9010 são os casos canônicos, ver README).

Credenciais: usuário `queryboard`, senha `queryboard`.

## Checklist

Cadastre a connection no app (`pnpm tauri dev`) conforme a seção "4.
Cadastrar a connection" do README, ou escreva um teste ad-hoc chamando
`OracleDriver` diretamente (mesmo padrão de `tests/pg_types.rs`, só que
rodado manualmente em vez de `#[ignore]` + testcontainers).

- [ ] **Conectar** — `Testar` na UI responde "conexão ok" (EasyConnect
      `host:porta/serviço`, sem `tnsnames.ora`).
- [ ] **Schema explorer** — expande e lista `departments`, `employees`,
      `timesheets`, `payroll_runs` com as colunas certas (nomes em
      maiúsculo, normalizados para minúsculo em `row.<coluna>` nas
      regras — CLAUDE.md §5).
- [ ] **`NUMBER` de alta precisão** — `SELECT gross_amount FROM
      payroll_runs WHERE payroll_id = :id` com `id = 9001` devolve
      `12500.00`. relacionar com `salary` de `employees` (`NUMBER(12,2)`)
      pra conferir que não passou por `f64`/arredondou.
- [ ] **`DATE` inclui hora** — `SELECT hire_date FROM employees WHERE
      employee_id = :id` com `id = 901` devolve um `CellValue::Timestamp`
      (não `Date`), refletindo que `DATE` do Oracle sempre carrega
      componente de hora.
- [ ] **`TIMESTAMP`** — `SELECT generated_at FROM payroll_runs WHERE
      payroll_id = :id` com `id = 9001` devolve timestamp correto; com
      `id = 9010` devolve `Null` (caso canônico: `generated_at` é NULL
      pra folha em rascunho).
- [ ] **Bind nomeado repetido** — uma consulta com `:id` usado duas vezes
      (ex.: `WHERE employee_id = :id OR department_id = :id`) só pede o
      parâmetro uma vez na UI e funciona (confirma que `bind_order` do
      `sql/params.rs`, que dedup a por nome, bate com o bind posicional
      que o driver ODPI-C espera).
- [ ] **Sessão somente leitura** — qualquer tentativa de escrita na mesma
      sessão (não exposta na UI, mas se testar via driver direto) falha
      com `ORA-01456`.
- [ ] **Cancelamento** — uma query propositalmente lenta (ex. produto
      cartesiano grande contra uma view do dicionário de dados, tipo
      `SELECT COUNT(*) FROM all_objects a, all_objects b`) cancelada no
      meio via `query_cancel` retorna erro em poucos segundos, não espera
      a query terminar sozinha.
- [ ] **Erro nunca vaza credencial** — forçar um erro de execução (ex.
      SQL contra tabela inexistente, que passa no guard mas falha no
      banco) e conferir que a mensagem devolvida pra UI não contém usuário
      nem senha.
- [ ] **Casos canônicos batem com a tabela do README** — rodar a consulta
      de exemplo contra `payroll_runs`/`timesheets`/`employees` e
      conferir visualmente os 10 casos (9001-9010) contra a tabela
      "Domínio Oracle" do `README.md`.

## Limitações conhecidas (documentar se mudar)

- `tnsnames.ora`/wallet **não validado** — só EasyConnect
  (`host:porta/serviço`). Se testar isso, atualizar `README.md` e este
  documento.
- `NUMBER` acima de ~28-29 dígitos significativos (teto do
  `rust_decimal`) é decodificado como string pelo driver e nunca convertido
  para `rust_decimal::Decimal` internamente — só um consumidor rio abaixo
  que tentar fazer esse parse teria problema, não o driver em si.
- `IntervalDS`/`IntervalYM` não têm decodificação dedicada — caem no
  fallback de texto genérico (`decode_cell`), não testado manualmente
  ainda porque a seed não usa `INTERVAL`.
