-- Fixture de teste local para o queryboard.
-- Espelha o cenário de investigação descrito em CLAUDE.md §1:
-- oferta -> produto -> envio (produto->loja) -> relação oferta x loja x produto.
--
-- Uso: ver "Guia de teste local" no README.md.
--
-- offer_id 5001-5005 são os casos "canônicos" documentados no README —
-- não mude o significado deles. 5006 em diante é só volume/variedade
-- extra pra explorar filtros, LIKE, datas, booleano etc.

DROP TABLE IF EXISTS offer_store_product;
DROP TABLE IF EXISTS product_shipments;
DROP TABLE IF EXISTS offers;
DROP TABLE IF EXISTS stores;
DROP TABLE IF EXISTS products;

CREATE TABLE products (
    product_id   INTEGER PRIMARY KEY,
    product_name TEXT NOT NULL,
    sku          TEXT NOT NULL,
    category     TEXT NOT NULL,
    base_price   NUMERIC(10,2) NOT NULL,
    active       BOOLEAN NOT NULL DEFAULT true
);

CREATE TABLE stores (
    store_id   INTEGER PRIMARY KEY,
    store_name TEXT NOT NULL,
    region     TEXT NOT NULL,
    city       TEXT NOT NULL
);

CREATE TABLE offers (
    offer_id          INTEGER PRIMARY KEY,
    product_id        INTEGER NOT NULL REFERENCES products(product_id),
    store_id          INTEGER NOT NULL REFERENCES stores(store_id),
    offer_status      INTEGER NOT NULL, -- 1=rascunho 3=agendada 5=decorrendo 7=encerrada
    start_date        DATE NOT NULL,
    discount_percent  NUMERIC(5,2),     -- NULL = sem desconto declarado
    notes             TEXT,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE product_shipments (
    shipment_id INTEGER PRIMARY KEY,
    product_id  INTEGER NOT NULL REFERENCES products(product_id),
    store_id    INTEGER NOT NULL REFERENCES stores(store_id),
    quantity    INTEGER NOT NULL,
    shipped_at  TIMESTAMPTZ NOT NULL
);

CREATE TABLE offer_store_product (
    offer_id   INTEGER NOT NULL REFERENCES offers(offer_id),
    store_id   INTEGER NOT NULL REFERENCES stores(store_id),
    product_id INTEGER NOT NULL REFERENCES products(product_id),
    price      NUMERIC(10,2) NOT NULL,
    PRIMARY KEY (offer_id, store_id, product_id)
);

-- ---------------------------------------------------------------------
-- products (12) — categorias e um produto inativo (active=false) pra
-- testar parâmetro booleano
-- ---------------------------------------------------------------------
INSERT INTO products (product_id, product_name, sku, category, base_price, active) VALUES
    (100, 'Fone Bluetooth X200',        'SKU-FONE-200',  'Áudio',            149.90,  true),
    (101, 'Mouse Gamer RGB',            'SKU-MOUSE-01',  'Periféricos',       89.90,  true),
    (102, 'Teclado Mecânico Compact',   'SKU-TECL-07',   'Periféricos',      259.90,  true),
    (103, 'Carregador USB-C 65W',       'SKU-CARR-65',   'Acessórios',        99.90,  true),
    (104, 'Caixa de Som Bluetooth',     'SKU-CAIXA-10',  'Áudio',            199.90,  true),
    (105, 'Monitor 27" 144Hz',          'SKU-MON-27',    'Informática',     1899.90,  true),
    (106, 'SSD NVMe 1TB',               'SKU-SSD-1TB',   'Informática',      549.90,  true),
    (107, 'Headset Gamer 7.1',          'SKU-HEAD-71',   'Áudio',            329.90,  true),
    (108, 'Webcam Full HD',             'SKU-CAM-FHD',   'Periféricos',      179.90,  true),
    (109, 'Lâmpada Inteligente Wi-Fi',  'SKU-LAMP-WIFI', 'Casa Inteligente',  79.90,  true),
    (110, 'Fechadura Digital',          'SKU-FECH-DIG',  'Casa Inteligente', 449.90,  false),
    (111, 'Cabo HDMI 2.1 2m',           'SKU-CABO-HDMI', 'Acessórios',        49.90,  true);

-- ---------------------------------------------------------------------
-- stores (6) — regiões diferentes
-- ---------------------------------------------------------------------
INSERT INTO stores (store_id, store_name, region, city) VALUES
    (10, 'Loja Centro',        'Sudeste',      'São Paulo'),
    (11, 'Loja Norte',         'Sudeste',      'São Paulo'),
    (12, 'Loja Shopping Sul',  'Sul',          'Porto Alegre'),
    (13, 'Loja Recife Centro', 'Nordeste',     'Recife'),
    (14, 'Loja Brasília Sul',  'Centro-Oeste', 'Brasília'),
    (15, 'Loja Curitiba Batel','Sul',          'Curitiba');

-- ---------------------------------------------------------------------
-- offers 5001-5005 — casos canônicos (não mudar o significado)
-- ---------------------------------------------------------------------

-- 5001: decorrendo (status 5), envio ok, preço batendo -> caminho feliz
INSERT INTO offers (offer_id, product_id, store_id, offer_status, start_date, discount_percent, notes) VALUES
    (5001, 100, 10, 5, '2026-07-20', 10.00, 'Campanha de inverno');
INSERT INTO product_shipments (shipment_id, product_id, store_id, quantity, shipped_at) VALUES
    (9001, 100, 10, 50, '2026-07-18T10:00:00Z');
INSERT INTO offer_store_product (offer_id, store_id, product_id, price) VALUES
    (5001, 10, 100, 149.90);

-- 5002: decorrendo (status 5), mas SEM envio -> caso "produto não enviado"
INSERT INTO offers (offer_id, product_id, store_id, offer_status, start_date, discount_percent, notes) VALUES
    (5002, 101, 11, 5, '2026-07-22', NULL, NULL);
INSERT INTO offer_store_product (offer_id, store_id, product_id, price) VALUES
    (5002, 11, 101, 89.90);

-- 5003: decorrendo (status 5), envio ok, mas preço divergente -> caso "preço divergente"
INSERT INTO offers (offer_id, product_id, store_id, offer_status, start_date, discount_percent, notes) VALUES
    (5003, 102, 12, 5, '2026-07-25', 5.00, NULL);
INSERT INTO product_shipments (shipment_id, product_id, store_id, quantity, shipped_at) VALUES
    (9002, 102, 12, 20, '2026-07-24T09:30:00Z');
INSERT INTO offer_store_product (offer_id, store_id, product_id, price) VALUES
    (5003, 12, 102, 239.90); -- base_price é 259.90

-- 5004: agendada (status 3) -> ainda não decorrendo
INSERT INTO offers (offer_id, product_id, store_id, offer_status, start_date, discount_percent, notes) VALUES
    (5004, 103, 10, 3, '2026-08-10', NULL, 'Aguardando aprovação de preço');

-- 5005: status desconhecido de propósito (99) -> caso "unmatched"
INSERT INTO offers (offer_id, product_id, store_id, offer_status, start_date, discount_percent, notes) VALUES
    (5005, 100, 11, 99, '2026-06-01', NULL, NULL);

-- ---------------------------------------------------------------------
-- offers 5006-5035 — volume extra pra testar filtro, LIKE, datas, região
-- ---------------------------------------------------------------------
INSERT INTO offers (offer_id, product_id, store_id, offer_status, start_date, discount_percent, notes) VALUES
    (5006, 104, 13, 5, '2026-05-03', 15.00, 'Lançamento regional'),
    (5007, 105, 14, 5, '2026-05-10', NULL, NULL),
    (5008, 106, 15, 7, '2026-04-01', 20.00, 'Encerrada — Black Friday antecipada'),
    (5009, 107, 10, 5, '2026-06-05', 8.50, NULL),
    (5010, 108, 11, 1, '2026-09-15', NULL, 'Rascunho, aguardando revisão'),
    (5011, 109, 12, 5, '2026-06-12', 12.00, NULL),
    (5012, 110, 13, 5, '2026-06-18', NULL, 'Produto marcado inativo no catálogo'),
    (5013, 111, 14, 3, '2026-08-01', 5.00, NULL),
    (5014, 100, 15, 5, '2026-06-20', NULL, NULL),
    (5015, 101, 10, 7, '2026-03-15', 25.00, 'Encerrada'),
    (5016, 102, 11, 5, '2026-07-01', NULL, NULL),
    (5017, 103, 12, 5, '2026-07-03', 10.00, NULL),
    (5018, 104, 13, 5, '2026-07-05', NULL, NULL),
    (5019, 105, 14, 99, '2026-05-20', NULL, 'Status legado, revisar'),
    (5020, 106, 15, 5, '2026-07-08', 18.00, NULL),
    (5021, 107, 10, 3, '2026-08-15', NULL, 'Agendada para o próximo ciclo'),
    (5022, 108, 11, 5, '2026-07-10', NULL, NULL),
    (5023, 109, 12, 5, '2026-07-11', 7.50, NULL),
    (5024, 110, 13, 1, '2026-09-01', NULL, 'Rascunho'),
    (5025, 111, 14, 5, '2026-07-14', NULL, NULL),
    (5026, 100, 15, 5, '2026-07-15', 10.00, NULL),
    (5027, 101, 10, 5, '2026-07-16', NULL, NULL),
    (5028, 102, 11, 7, '2026-02-28', 30.00, 'Encerrada há mais tempo'),
    (5029, 103, 12, 5, '2026-07-19', NULL, NULL),
    (5030, 104, 13, 5, '2026-07-21', 22.00, 'Combo com fone'),
    (5031, 105, 14, 5, '2026-07-23', NULL, NULL),
    (5032, 106, 15, 3, '2026-08-20', NULL, 'Agendada'),
    (5033, 107, 10, 5, '2026-07-26', 9.90, NULL),
    (5034, 108, 11, 5, '2026-07-27', NULL, NULL),
    (5035, 109, 12, 99, '2026-04-15', NULL, 'Status legado');

-- Envios: a maioria recebe envio, alguns propositalmente ficam sem
-- (5010, 5019, 5024, 5028, 5032, 5035) pra continuar exercitando a regra
-- "produto não enviado" com mais exemplos.
INSERT INTO product_shipments (shipment_id, product_id, store_id, quantity, shipped_at) VALUES
    (9003, 104, 13, 30, '2026-05-01T08:00:00Z'),
    (9004, 105, 14,  8, '2026-05-08T08:00:00Z'),
    (9005, 106, 15, 40, '2026-03-28T08:00:00Z'),
    (9006, 107, 10, 25, '2026-06-02T08:00:00Z'),
    (9007, 109, 12, 60, '2026-06-10T08:00:00Z'),
    (9008, 110, 13, 15, '2026-06-15T08:00:00Z'),
    (9009, 111, 14, 90, '2026-07-28T08:00:00Z'),
    (9010, 100, 15, 45, '2026-06-18T08:00:00Z'),
    (9011, 101, 10, 35, '2026-03-10T08:00:00Z'),
    (9012, 102, 11, 28, '2026-06-29T08:00:00Z'),
    (9013, 103, 12, 50, '2026-07-01T08:00:00Z'),
    (9014, 104, 13, 20, '2026-07-03T08:00:00Z'),
    (9015, 106, 15, 33, '2026-07-06T08:00:00Z'),
    (9016, 108, 11, 22, '2026-07-08T08:00:00Z'),
    (9017, 109, 12, 44, '2026-07-09T08:00:00Z'),
    (9018, 111, 14, 18, '2026-07-12T08:00:00Z'),
    (9019, 100, 15, 26, '2026-07-13T08:00:00Z'),
    (9020, 101, 10, 31, '2026-07-14T08:00:00Z'),
    (9021, 102, 11, 12, '2026-02-25T08:00:00Z'),
    (9022, 103, 12, 27, '2026-07-17T08:00:00Z'),
    (9023, 104, 13, 19, '2026-07-19T08:00:00Z'),
    (9024, 105, 14, 24, '2026-07-21T08:00:00Z'),
    (9025, 107, 10, 16, '2026-07-24T08:00:00Z'),
    (9026, 108, 11, 29, '2026-07-25T08:00:00Z');

-- Preço praticado — a maioria bate com base_price; 5006, 5017, 5030
-- ficam com divergência proposital pra testar a regra de preço divergente
-- com mais exemplos além do 5003.
INSERT INTO offer_store_product (offer_id, store_id, product_id, price) VALUES
    (5006, 13, 104, 169.90), -- base 199.90, divergente
    (5007, 14, 105, 1899.90),
    (5008, 15, 106, 549.90),
    (5009, 10, 107, 329.90),
    (5010, 11, 108, 179.90),
    (5011, 12, 109, 79.90),
    (5012, 13, 110, 449.90),
    (5013, 14, 111, 49.90),
    (5014, 15, 100, 149.90),
    (5015, 10, 101, 89.90),
    (5016, 11, 102, 259.90),
    (5017, 12, 103, 79.90), -- base 99.90, divergente
    (5018, 13, 104, 199.90),
    (5020, 15, 106, 549.90),
    (5022, 11, 108, 179.90),
    (5023, 12, 109, 79.90),
    (5025, 14, 111, 49.90),
    (5026, 15, 100, 149.90),
    (5027, 10, 101, 89.90),
    (5029, 12, 103, 99.90),
    (5030, 13, 104, 179.90), -- base 199.90, divergente
    (5031, 14, 105, 1899.90),
    (5033, 10, 107, 329.90),
    (5034, 11, 108, 179.90);

-- =======================================================================
-- SEGUNDO CASO DE INVESTIGAÇÃO — pedidos, pagamentos e entrega
--
-- Domínio diferente do de ofertas acima, pro mesmo padrão de investigação
-- do CLAUDE.md §1: cliente -> pedido -> pagamento -> envio, com o mesmo
-- tipo de furo que você teria que caçar manualmente num incidente real
-- (pedido "pago" sem registro de pagamento, valor pago divergente do
-- total, pedido "enviado" sem registro de envio, cancelado sem estorno,
-- parado em trânsito, status sem significado conhecido). Reusa a tabela
-- `products` já semeada acima pros itens do pedido.
--
-- order_id 6001-6010 são os casos canônicos (não mude o significado).
-- 6011 em diante é volume/variedade extra.
-- =======================================================================

DROP TABLE IF EXISTS order_shipments;
DROP TABLE IF EXISTS payments;
DROP TABLE IF EXISTS order_items;
DROP TABLE IF EXISTS orders;
DROP TABLE IF EXISTS customers;

CREATE TABLE customers (
    customer_id INTEGER PRIMARY KEY,
    full_name   TEXT NOT NULL,
    email       TEXT NOT NULL,
    segment     TEXT NOT NULL, -- 'novo' | 'recorrente' | 'vip'
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE orders (
    order_id     INTEGER PRIMARY KEY,
    customer_id  INTEGER NOT NULL REFERENCES customers(customer_id),
    order_status INTEGER NOT NULL, -- 1=criado 2=pago 3=faturado 4=enviado 5=entregue 9=cancelado
    order_date   DATE NOT NULL,
    total_amount NUMERIC(10,2) NOT NULL
);

CREATE TABLE order_items (
    order_item_id INTEGER PRIMARY KEY,
    order_id      INTEGER NOT NULL REFERENCES orders(order_id),
    product_id    INTEGER NOT NULL REFERENCES products(product_id),
    quantity      INTEGER NOT NULL,
    unit_price    NUMERIC(10,2) NOT NULL
);

CREATE TABLE payments (
    payment_id     INTEGER PRIMARY KEY,
    order_id       INTEGER NOT NULL REFERENCES orders(order_id),
    payment_status INTEGER NOT NULL, -- 1=pendente 2=aprovado 3=recusado 4=estornado
    amount         NUMERIC(10,2) NOT NULL,
    paid_at        TIMESTAMPTZ
);

CREATE TABLE order_shipments (
    order_shipment_id INTEGER PRIMARY KEY,
    order_id          INTEGER NOT NULL REFERENCES orders(order_id),
    carrier           TEXT NOT NULL,
    tracking_code     TEXT,
    shipped_at        TIMESTAMPTZ,
    delivered_at      TIMESTAMPTZ
);

INSERT INTO customers (customer_id, full_name, email, segment) VALUES
    (200, 'Ana Ribeiro',       'ana.ribeiro@example.com',       'vip'),
    (201, 'Bruno Castro',      'bruno.castro@example.com',      'recorrente'),
    (202, 'Carla Menezes',     'carla.menezes@example.com',     'novo'),
    (203, 'Diego Almeida',     'diego.almeida@example.com',     'recorrente'),
    (204, 'Elisa Prado',       'elisa.prado@example.com',       'vip'),
    (205, 'Felipe Nogueira',   'felipe.nogueira@example.com',   'novo'),
    (206, 'Gabriela Torres',   'gabriela.torres@example.com',   'recorrente'),
    (207, 'Hugo Barreto',      'hugo.barreto@example.com',      'novo'),
    (208, 'Isabela Farias',    'isabela.farias@example.com',    'vip'),
    (209, 'João Vitor Souza',  'joao.souza@example.com',        'recorrente');

-- -----------------------------------------------------------------------
-- 6001-6010 — casos canônicos
-- -----------------------------------------------------------------------

-- 6001: caminho feliz — pago, itens batem com o total, enviado e entregue
-- (2x Fone 149.90 + 1x Cabo HDMI 49.90 = 349.70, igual total_amount e amount)
INSERT INTO orders (order_id, customer_id, order_status, order_date, total_amount) VALUES
    (6001, 200, 5, '2026-06-10', 349.70);
INSERT INTO order_items (order_item_id, order_id, product_id, quantity, unit_price) VALUES
    (7001, 6001, 100, 2, 149.90), (7002, 6001, 111, 1, 49.90);
INSERT INTO payments (payment_id, order_id, payment_status, amount, paid_at) VALUES
    (8001, 6001, 2, 349.70, '2026-06-10T14:00:00Z');
INSERT INTO order_shipments (order_shipment_id, order_id, carrier, tracking_code, shipped_at, delivered_at) VALUES
    (9101, 6001, 'Correios', 'BR123456789', '2026-06-11T09:00:00Z', '2026-06-15T16:20:00Z');

-- 6002: pago, mas valor pago DIVERGENTE do total do pedido
INSERT INTO orders (order_id, customer_id, order_status, order_date, total_amount) VALUES
    (6002, 201, 2, '2026-06-15', 259.90);
INSERT INTO order_items (order_item_id, order_id, product_id, quantity, unit_price) VALUES
    (7003, 6002, 102, 1, 259.90);
INSERT INTO payments (payment_id, order_id, payment_status, amount, paid_at) VALUES
    (8002, 6002, 2, 199.90, '2026-06-15T10:15:00Z'); -- deveria ser 259.90

-- 6003: status "enviado", mas SEM registro em order_shipments
INSERT INTO orders (order_id, customer_id, order_status, order_date, total_amount) VALUES
    (6003, 202, 4, '2026-06-18', 89.90);
INSERT INTO order_items (order_item_id, order_id, product_id, quantity, unit_price) VALUES
    (7004, 6003, 101, 1, 89.90);
INSERT INTO payments (payment_id, order_id, payment_status, amount, paid_at) VALUES
    (8003, 6003, 2, 89.90, '2026-06-18T08:40:00Z');

-- 6004: status "pago", mas SEM registro em payments
INSERT INTO orders (order_id, customer_id, order_status, order_date, total_amount) VALUES
    (6004, 203, 2, '2026-06-20', 179.90);
INSERT INTO order_items (order_item_id, order_id, product_id, quantity, unit_price) VALUES
    (7005, 6004, 108, 1, 179.90);

-- 6005: enviado há tempo, mas nunca chegou a "entregue" — parado em trânsito
INSERT INTO orders (order_id, customer_id, order_status, order_date, total_amount) VALUES
    (6005, 204, 4, '2026-05-01', 549.90);
INSERT INTO order_items (order_item_id, order_id, product_id, quantity, unit_price) VALUES
    (7006, 6005, 106, 1, 549.90);
INSERT INTO payments (payment_id, order_id, payment_status, amount, paid_at) VALUES
    (8004, 6005, 2, 549.90, '2026-05-01T11:00:00Z');
INSERT INTO order_shipments (order_shipment_id, order_id, carrier, tracking_code, shipped_at, delivered_at) VALUES
    (9102, 6005, 'Transportadora XPTO', 'XPTO998877', '2026-05-03T07:30:00Z', NULL);

-- 6006: cancelado, mas o pagamento continua "aprovado" (não estornado)
INSERT INTO orders (order_id, customer_id, order_status, order_date, total_amount) VALUES
    (6006, 205, 9, '2026-06-22', 99.90);
INSERT INTO order_items (order_item_id, order_id, product_id, quantity, unit_price) VALUES
    (7007, 6006, 103, 1, 99.90);
INSERT INTO payments (payment_id, order_id, payment_status, amount, paid_at) VALUES
    (8005, 6006, 2, 99.90, '2026-06-22T09:05:00Z'); -- deveria estar estornado (4)

-- 6007: status sem significado conhecido (99) — caso "unmatched"
INSERT INTO orders (order_id, customer_id, order_status, order_date, total_amount) VALUES
    (6007, 206, 99, '2026-04-30', 149.90);
INSERT INTO order_items (order_item_id, order_id, product_id, quantity, unit_price) VALUES
    (7008, 6007, 100, 1, 149.90);

-- 6008: recém-criado, ainda não pago — caminho normal inicial
INSERT INTO orders (order_id, customer_id, order_status, order_date, total_amount) VALUES
    (6008, 207, 1, '2026-07-28', 259.90);
INSERT INTO order_items (order_item_id, order_id, product_id, quantity, unit_price) VALUES
    (7009, 6008, 102, 1, 259.90);

-- 6009: faturado e pago, mas SEM nenhum item — pedido "vazio"
INSERT INTO orders (order_id, customer_id, order_status, order_date, total_amount) VALUES
    (6009, 208, 3, '2026-06-25', 0.00);
INSERT INTO payments (payment_id, order_id, payment_status, amount, paid_at) VALUES
    (8006, 6009, 2, 0.00, '2026-06-25T13:00:00Z');

-- 6010: status do pedido diz "pago", mas o pagamento em si foi RECUSADO
INSERT INTO orders (order_id, customer_id, order_status, order_date, total_amount) VALUES
    (6010, 209, 2, '2026-06-27', 449.90);
INSERT INTO order_items (order_item_id, order_id, product_id, quantity, unit_price) VALUES
    (7010, 6010, 110, 1, 449.90);
INSERT INTO payments (payment_id, order_id, payment_status, amount, paid_at) VALUES
    (8007, 6010, 3, 449.90, NULL); -- recusado, sem paid_at

-- -----------------------------------------------------------------------
-- 6011-6030 — volume extra pra explorar filtro por cliente, segmento,
-- data e status à vontade
-- -----------------------------------------------------------------------
INSERT INTO orders (order_id, customer_id, order_status, order_date, total_amount) VALUES
    (6011, 200, 5, '2026-05-05', 179.90),
    (6012, 201, 5, '2026-05-12', 89.90),
    (6013, 202, 4, '2026-07-01', 549.90),
    (6014, 203, 2, '2026-07-02', 329.90),
    (6015, 204, 5, '2026-05-20', 99.90),
    (6016, 205, 1, '2026-07-29', 149.90),
    (6017, 206, 5, '2026-06-01', 259.90),
    (6018, 207, 5, '2026-06-03', 79.90),
    (6019, 208, 4, '2026-07-15', 199.90),
    (6020, 209, 5, '2026-06-08', 49.90),
    (6021, 200, 3, '2026-07-18', 449.90),
    (6022, 201, 5, '2026-06-11', 1899.90),
    (6023, 202, 2, '2026-07-20', 89.90),
    (6024, 203, 5, '2026-06-14', 149.90),
    (6025, 204, 9, '2026-04-10', 259.90),
    (6026, 205, 5, '2026-06-17', 329.90),
    (6027, 206, 1, '2026-07-30', 99.90),
    (6028, 207, 5, '2026-06-21', 179.90),
    (6029, 208, 5, '2026-06-24', 549.90),
    (6030, 209, 5, '2026-06-28', 89.90);

INSERT INTO order_items (order_item_id, order_id, product_id, quantity, unit_price) VALUES
    (7011, 6011, 108, 1, 179.90), (7012, 6012, 101, 1, 89.90),
    (7013, 6013, 106, 1, 549.90), (7014, 6014, 107, 1, 329.90),
    (7015, 6015, 103, 1, 99.90),  (7016, 6016, 100, 1, 149.90),
    (7017, 6017, 102, 1, 259.90), (7018, 6018, 109, 1, 79.90),
    (7019, 6019, 104, 1, 199.90), (7020, 6020, 111, 1, 49.90),
    (7021, 6021, 110, 1, 449.90), (7022, 6022, 105, 1, 1899.90),
    (7023, 6023, 101, 1, 89.90),  (7024, 6024, 100, 1, 149.90),
    (7025, 6025, 102, 1, 259.90), (7026, 6026, 107, 1, 329.90),
    (7027, 6027, 103, 1, 99.90),  (7028, 6028, 108, 1, 179.90),
    (7029, 6029, 106, 1, 549.90), (7030, 6030, 101, 1, 89.90);

-- Pagamento aprovado pra todo pedido com status >= 2 (pago), exceto os
-- que já viraram cancelado sem chegar a pagar de fato (nenhum aqui).
INSERT INTO payments (payment_id, order_id, payment_status, amount, paid_at) VALUES
    (8011, 6011, 2, 179.90, '2026-05-05T10:00:00Z'),
    (8012, 6012, 2, 89.90,  '2026-05-12T10:00:00Z'),
    (8013, 6013, 2, 549.90, '2026-07-01T10:00:00Z'),
    (8014, 6014, 2, 329.90, '2026-07-02T10:00:00Z'),
    (8015, 6015, 2, 99.90,  '2026-05-20T10:00:00Z'),
    (8017, 6017, 2, 259.90, '2026-06-01T10:00:00Z'),
    (8018, 6018, 2, 79.90,  '2026-06-03T10:00:00Z'),
    (8019, 6019, 2, 199.90, '2026-07-15T10:00:00Z'),
    (8020, 6020, 2, 49.90,  '2026-06-08T10:00:00Z'),
    (8021, 6021, 2, 449.90, '2026-07-18T10:00:00Z'),
    (8022, 6022, 2, 1899.90,'2026-06-11T10:00:00Z'),
    (8023, 6023, 2, 89.90,  '2026-07-20T10:00:00Z'),
    (8024, 6024, 2, 149.90, '2026-06-14T10:00:00Z'),
    (8026, 6026, 2, 329.90, '2026-06-17T10:00:00Z'),
    (8028, 6028, 2, 179.90, '2026-06-21T10:00:00Z'),
    (8029, 6029, 2, 549.90, '2026-06-24T10:00:00Z'),
    (8030, 6030, 2, 89.90,  '2026-06-28T10:00:00Z');

-- Envio pra todo pedido "enviado" (4) ou "entregue" (5), com entrega só
-- pros "entregue" — os "enviado" (6013, 6019) ficam parados em trânsito
-- de propósito, igual o 6005.
INSERT INTO order_shipments (order_shipment_id, order_id, carrier, tracking_code, shipped_at, delivered_at) VALUES
    (9111, 6011, 'Correios', 'BR100000011', '2026-05-06T09:00:00Z', '2026-05-10T15:00:00Z'),
    (9112, 6012, 'Correios', 'BR100000012', '2026-05-13T09:00:00Z', '2026-05-17T15:00:00Z'),
    (9113, 6013, 'Transportadora XPTO', 'XPTO000013', '2026-07-02T09:00:00Z', NULL),
    (9115, 6015, 'Correios', 'BR100000015', '2026-05-21T09:00:00Z', '2026-05-25T15:00:00Z'),
    (9117, 6017, 'Correios', 'BR100000017', '2026-06-02T09:00:00Z', '2026-06-06T15:00:00Z'),
    (9118, 6018, 'Correios', 'BR100000018', '2026-06-04T09:00:00Z', '2026-06-08T15:00:00Z'),
    (9119, 6019, 'Transportadora XPTO', 'XPTO000019', '2026-07-16T09:00:00Z', NULL),
    (9120, 6020, 'Correios', 'BR100000020', '2026-06-09T09:00:00Z', '2026-06-13T15:00:00Z'),
    (9122, 6022, 'Correios', 'BR100000022', '2026-06-12T09:00:00Z', '2026-06-16T15:00:00Z'),
    (9124, 6024, 'Correios', 'BR100000024', '2026-06-15T09:00:00Z', '2026-06-19T15:00:00Z'),
    (9126, 6026, 'Correios', 'BR100000026', '2026-06-18T09:00:00Z', '2026-06-22T15:00:00Z'),
    (9128, 6028, 'Correios', 'BR100000028', '2026-06-22T09:00:00Z', '2026-06-26T15:00:00Z'),
    (9129, 6029, 'Correios', 'BR100000029', '2026-06-25T09:00:00Z', '2026-06-29T15:00:00Z'),
    (9130, 6030, 'Correios', 'BR100000030', '2026-06-29T09:00:00Z', '2026-07-03T15:00:00Z');
