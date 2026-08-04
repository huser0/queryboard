# queryboard

App desktop leve, multi-banco e somente leitura para executar SELECTs e interpretar os resultados automaticamente. Ver `CLAUDE.MD` para a visão completa do projeto.

## Recommended IDE Setup

- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)

## Guia de teste local

Passo a passo para levantar bancos de teste (Postgres, MySQL e, opcionalmente, Oracle) e rodar o app ponta a ponta.

### 1. Pré-requisitos

- `pnpm install` já rodado na raiz do projeto.
- Docker ou Podman (o `docker` abaixo funciona com qualquer um dos dois).
- **Rust**, via [rustup](https://rustup.rs/):
  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  source "$HOME/.cargo/env"   # ou abra um terminal novo
  ```
- **Libs de sistema do Tauri v2** (GTK/WebKit) — sem elas o `cargo
  build`/`pnpm tauri dev` falha já no link, antes de chegar a abrir
  janela nenhuma:
  - Fedora/RHEL (`dnf`):
    ```bash
    sudo dnf install webkit2gtk4.1-devel openssl-devel curl wget file \
      libappindicator-gtk3-devel librsvg2-devel
    sudo dnf group install "c-development"
    ```
  - Debian/Ubuntu (`apt`):
    ```bash
    sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file \
      libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
    ```
- **No WSL2:** o app abre uma janela nativa via WSLg. Se `pnpm tauri dev` falhar com `Failed to initialize GTK backend`, o terminal está sem as variáveis do WSLg — abra um terminal novo (ou rode `source ~/.bashrc`) para pegar as exportações de `DISPLAY`/`WAYLAND_DISPLAY`/`XDG_RUNTIME_DIR` já configuradas no `.bashrc`.
- **Em uma `podman machine` (VM headless via `podman machine ssh`):** o mesmo erro `Failed to initialize GTK backend` acontece porque a VM não herda `DISPLAY`/`WAYLAND_DISPLAY`/`XDG_RUNTIME_DIR` do host — dentro dela essas variáveis vêm vazias (ou `XDG_RUNTIME_DIR` aponta pra `/run/user/0`, se você estiver como root). É preciso fazer X11 forwarding via SSH manualmente (Wayland não é network-transparent, então X11 é o caminho viável aqui — GTK cai pra XWayland sem problema mesmo com um desktop Wayland no host):
  ```bash
  # dentro da VM: garantir xauth instalado e X11Forwarding habilitado no sshd
  podman machine ssh -- sudo dnf install -y xorg-x11-xauth
  podman machine ssh -- sudo sh -c \
    'grep -qi "^X11Forwarding yes" /etc/ssh/sshd_config || echo "X11Forwarding yes" >> /etc/ssh/sshd_config'
  podman machine ssh -- sudo systemctl restart sshd

  # pegar porta/chave da conexão gerada pelo podman machine
  podman system connection list

  # conectar com -X (substitua porta/chave/usuário pelos dados acima)
  ssh -X -p <porta> -i <arquivo-de-identidade> <usuario>@localhost

  # nessa sessão com -X, $DISPLAY deve aparecer preenchido (ex: localhost:10.0)
  echo "$DISPLAY"

  # e então, na mesma sessão:
  pnpm tauri dev
  ```
  Esses passos ainda não foram validados contra uma podman machine real — se `X11Forwarding` já vier habilitado por padrão, ou a porta/usuário forem diferentes, ajuste conforme necessário.

### 2. Subir os bancos de teste

```bash
docker compose up -d --wait
```

O `docker-compose.yml` sobe Postgres e MySQL juntos, espera os healthchecks
ficarem `healthy` e popula cada um com sua seed (`dev/seed-postgres.sql` /
`dev/seed-mysql.sql`) automaticamente na primeira inicialização — nenhum
passo manual extra. Postgres fica em `localhost:55432`, MySQL em
`localhost:53306`.

Oracle fica **atrás de um profile** — o startup do Oracle Database Free é
lento (~1-2 min) e, seguindo o `CLAUDE.md`, a validação dele é manual, não
automatizada, então ele não atrasa quem só quer Postgres/MySQL:

```bash
docker compose --profile oracle up -d --wait
```

Sobe em `localhost:51521`, serviço `FREEPDB1`, populado com
`dev/seed-oracle.sql`.

Os dados semeados (`dev/seed-postgres.sql`) espelham o cenário de investigação do `CLAUDE.MD` (oferta → produto → envio → relação oferta×loja×produto): 12 produtos (com `category` e `active`), 6 lojas (com `region`/`city`) e 35 ofertas (com `discount_percent` e `notes`) — dá pra testar filtro por texto (`LIKE`), número, data, e booleano. Os cinco primeiros IDs são os casos canônicos, propositalmente quebrados:

| offer_id | offer_status | Situação |
|---|---|---|
| 5001 | 5 (decorrendo) | caminho feliz: envio ok, preço batendo |
| 5002 | 5 (decorrendo) | **sem envio registrado** |
| 5003 | 5 (decorrendo) | envio ok, mas **preço divergente** (239.90 vs base 259.90) |
| 5004 | 3 (agendada) | ainda não decorrendo |
| 5005 | 99 | status sem significado conhecido |

`5006`-`5035` têm mais variedade (datas de maio a setembro/2026, todas as regiões, alguns sem envio, alguns com preço divergente, um produto inativo — `product_id 110`) pra explorar filtros à vontade.

**Segundo caso, domínio diferente:** `customers` → `orders` → `order_items` → `payments` → `order_shipments` (pedido/pagamento/entrega), reusando a tabela `products` já semeada. `order_id` 6001-6010 são os canônicos:

| order_id | order_status | Situação |
|---|---|---|
| 6001 | 5 (entregue) | caminho feliz: itens batem com total, pago, entregue |
| 6002 | 2 (pago) | **valor pago divergente** do total (199.90 vs 259.90) |
| 6003 | 4 (enviado) | **sem registro de envio** |
| 6004 | 2 (pago) | **sem registro de pagamento** |
| 6005 | 4 (enviado) | enviado, mas **nunca chegou a "entregue"** (parado em trânsito) |
| 6006 | 9 (cancelado) | pagamento continua **aprovado, não estornado** |
| 6007 | 99 | status sem significado conhecido |
| 6008 | 1 (criado) | ainda não pago — caminho normal inicial |
| 6009 | 3 (faturado) | pago, mas **sem nenhum item** (pedido vazio) |
| 6010 | 2 (pago) | status diz pago, mas o pagamento em si foi **recusado** (`payment_status = 3`) |

`6011`-`6030` são volume extra pra filtrar por cliente, segmento, data e status.

**Domínio MySQL, diferente do Postgres:** `agents` → `customers` →
`support_tickets` → `ticket_events` (helpdesk de suporte). `ticket_id`
8001-8010 são os canônicos:

| ticket_id | status | Situação |
|---|---|---|
| 8001 | 3 (resolvido) | caminho feliz: aberto → em andamento → resolvido, dentro do SLA |
| 8002 | 3 (resolvido) | **`resolved_at` NULL** apesar do status |
| 8003 | 3 (resolvido) | **SLA estourado** (`resolved_at` - `created_at` > `sla_minutes`) |
| 8004 | 4 (reaberto) | reaberto, mas **sem agente atribuído** |
| 8005 | 2 (em andamento) | **nunca teve evento de atribuição** de agente |
| 8006 | 1 (aberto) | prioridade **sem significado conhecido** (99) |
| 8007 | 3 (resolvido) | **sem nenhum `ticket_events`** — resolução "fantasma" |
| 8008 | 1 (aberto) | ainda aberto, dentro do SLA — caminho normal em andamento |
| 8009 | 5 (fechado) | fechado **sem nunca ter passado por "resolvido"** |
| 8010 | 1 (aberto) | cliente com **plano cancelado**, ticket aberto depois do cancelamento |

`8011`-`8040` são volume extra pra filtrar por time, prioridade, data e status.

**Domínio Oracle, diferente dos dois acima:** `departments` → `employees`
→ `timesheets` → `payroll_runs` (RH e folha de pagamento). `payroll_id`
9001-9010 são os canônicos:

| payroll_id | status | Situação |
|---|---|---|
| 9001 | 3 (pago) | caminho feliz: timesheet aprovado, folha bate com o salário |
| 9002 | 3 (pago) | horas lançadas **sem aprovação** (`approved = 'N'`), folha gerada mesmo assim |
| 9003 | 3 (pago) | `gross_amount` **divergente** do salário do funcionário |
| 9004 | 3 (pago) | funcionário **desligado** antes do período da folha |
| 9005 | 3 (pago) | funcionário de departamento **sem gestor** (`manager_employee_id` NULL) |
| 9006 | 99 | status **sem significado conhecido** |
| 9007 | 3 (pago) | **sem nenhum `timesheets`** no período — folha "fantasma" |
| 9008 | *(sem folha)* | timesheet lançado, **ainda sem folha gerada** — caminho normal em andamento |
| 9009 | 3 (pago) | pago **sem timesheet aprovado** no período |
| 9010 | 1 (rascunho) | rascunho, **sem nenhum `timesheets`** lançado no período |

`9011`-`9040` são volume extra pra filtrar por departamento, status, data e valor.

### 3. Rodar o app

```bash
pnpm tauri dev
```

Isso compila o backend Rust e abre a janela do app (primeira vez demora alguns minutos por causa da compilação).

### 4. Cadastrar a connection

Na **barra lateral esquerda**, clique em **+ Nova connection** e preencha:

| Campo | Valor |
|---|---|
| Slug | `queryboard_local` |
| Nome | `Postgres Local (Docker)` |
| Tipo | `postgres` |
| Host | `localhost` |
| Porta | `55432` |
| Banco | `queryboard` |
| Usuário | `queryboard` |
| Senha | `queryboard` |

Pra testar MySQL, cadastre uma segunda connection:

| Campo | Valor |
|---|---|
| Slug | `queryboard_mysql` |
| Nome | `MySQL Local (Docker)` |
| Tipo | `mysql` |
| Host | `localhost` |
| Porta | `53306` |
| Banco | `queryboard` |
| Usuário | `queryboard` |
| Senha | `queryboard` |

E, se tiver subido o profile do Oracle (passo 2), uma terceira:

| Campo | Valor |
|---|---|
| Slug | `queryboard_oracle` |
| Nome | `Oracle Local (Docker)` |
| Tipo | `oracle` |
| Host | `localhost` |
| Porta | `51521` |
| Banco | `FREEPDB1` |
| Usuário | `queryboard` |
| Senha | `queryboard` |

O driver Oracle conecta via EasyConnect (`host:porta/serviço`, usando o
campo "Banco" como nome do serviço) — não depende de `tnsnames.ora`. Se o
seu ambiente de produção usa `tnsnames.ora`/wallet em vez de EasyConnect,
isso ainda não foi validado neste projeto (só o caminho EasyConnect foi
testado, contra o Oracle Database Free local).

Salve, clique em **Testar** (deve responder "conexão ok"), e depois clique no nome da connection na lista para deixá-la **ativa** — ela fica destacada na barra lateral e aparece no cabeçalho "Conectado a: ..." no topo do painel.

### 5. Escrever e rodar uma consulta

No painel à direita, clique em **Adicionar SQL ad-hoc** — abre um bloco com editor próprio (a connection ativa já vem pré-selecionada, mas dá pra trocar). Cole um SQL com parâmetro nomeado (nunca concatenar valor, sempre bind):

```sql
SELECT o.offer_id, o.offer_status, o.start_date, p.product_name, s.store_name
FROM offers o
JOIN products p ON p.product_id = o.product_id
JOIN stores s ON s.store_id = o.store_id
WHERE o.offer_id = :offer_id
```

O campo de parâmetro `offer_id` aparece automaticamente acima do bloco (extraído da SQL). Preencha com `5002` ou `5003` para ver os casos de divergência descritos acima, e clique em **Executar** — o resultado aparece dentro do próprio bloco, logo abaixo.

Não precisa salvar pra rodar. Se quiser reusar a consulta depois, dê um slug no campo "slug para salvar" e clique em **Salvar** — ela passa a aparecer na lista de "Queries do painel", com um botão **Remover** próprio.

Pra rodar **várias consultas em paralelo**, clique em **Adicionar SQL ad-hoc** de novo (ou marque queries já salvas) e depois em **Executar tudo** — parâmetros com o mesmo nome entre elas são preenchidos uma única vez.

### Encerrar

```bash
docker compose stop   # pausa, mantém os dados pra próxima vez
docker compose down   # remove os containers e os dados (recomeça do zero na próxima subida)

# se tiver subido o Oracle, ele fica de fora dos comandos acima (profile
# separado) — inclua --profile oracle pra também parar/remover ele:
docker compose --profile oracle down
```
