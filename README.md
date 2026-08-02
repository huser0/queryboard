# queryboard

App desktop leve, multi-banco e somente leitura para executar SELECTs e interpretar os resultados automaticamente. Ver `CLAUDE.MD` para a visão completa do projeto.

## Recommended IDE Setup

- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)

## Guia de teste local

Passo a passo para levantar um Postgres de teste e rodar o app ponta a ponta.

### 1. Pré-requisitos

- `pnpm install` já rodado na raiz do projeto.
- Docker ou Podman (o `docker` abaixo funciona com qualquer um dos dois).
- **No WSL2:** o app abre uma janela nativa via WSLg. Se `pnpm tauri dev` falhar com `Failed to initialize GTK backend`, o terminal está sem as variáveis do WSLg — abra um terminal novo (ou rode `source ~/.bashrc`) para pegar as exportações de `DISPLAY`/`WAYLAND_DISPLAY`/`XDG_RUNTIME_DIR` já configuradas no `.bashrc`.

### 2. Subir o Postgres de teste

```bash
docker run -d --name queryboard-postgres \
  -e POSTGRES_USER=queryboard \
  -e POSTGRES_PASSWORD=queryboard \
  -e POSTGRES_DB=queryboard \
  -p 55432:5432 \
  postgres:16-alpine -c fsync=off

# espera o banco aceitar conexões
until docker exec queryboard-postgres pg_isready -U queryboard; do sleep 1; done

# popula com os dados de teste
docker exec -i queryboard-postgres psql -U queryboard -d queryboard < dev/seed-postgres.sql
```

O container fica ouvindo em `localhost:55432`. Para recomeçar do zero: `docker rm -f queryboard-postgres` e repita os três comandos.

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
docker stop queryboard-postgres   # ou `docker rm -f queryboard-postgres` para descartar os dados
```
