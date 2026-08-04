-- Fixture de teste local para o queryboard — domínio MySQL, diferente do
-- Postgres (oferta/pedido): helpdesk de suporte.
-- cliente -> ticket -> eventos do ticket, com agente atribuído.
--
-- Uso: ver "Guia de teste local" no README.md.
--
-- ticket_id 8001-8010 são os casos "canônicos" documentados no README —
-- não mude o significado deles. 8011 em diante é só volume/variedade
-- extra pra explorar filtros, LIKE, datas, prioridade etc.

DROP TABLE IF EXISTS ticket_events;
DROP TABLE IF EXISTS support_tickets;
DROP TABLE IF EXISTS customers;
DROP TABLE IF EXISTS agents;

CREATE TABLE agents (
    agent_id   INT PRIMARY KEY,
    agent_name VARCHAR(120) NOT NULL,
    team       VARCHAR(60) NOT NULL,
    active     BOOLEAN NOT NULL DEFAULT TRUE
);

CREATE TABLE customers (
    customer_id  INT PRIMARY KEY,
    company_name VARCHAR(160) NOT NULL,
    plan_status  VARCHAR(20) NOT NULL, -- 'active' | 'cancelled'
    created_at   TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE support_tickets (
    ticket_id    INT PRIMARY KEY,
    customer_id  INT NOT NULL REFERENCES customers(customer_id),
    agent_id     INT REFERENCES agents(agent_id), -- NULL = não atribuído
    subject      VARCHAR(200) NOT NULL,
    priority     INT NOT NULL, -- 1=baixa 2=média 3=alta
    status       INT NOT NULL, -- 1=aberto 2=em_andamento 3=resolvido 4=reaberto 5=fechado
    sla_minutes  INT NOT NULL,
    created_at   DATETIME NOT NULL,
    resolved_at  DATETIME,
    closed_at    DATETIME
);

CREATE TABLE ticket_events (
    event_id   INT PRIMARY KEY,
    ticket_id  INT NOT NULL REFERENCES support_tickets(ticket_id),
    event_type VARCHAR(30) NOT NULL, -- created|assigned|status_changed|reopened|resolved|closed|comment
    agent_id   INT REFERENCES agents(agent_id),
    created_at DATETIME NOT NULL
);

-- ---------------------------------------------------------------------
-- agents (8) — times diferentes
-- ---------------------------------------------------------------------
INSERT INTO agents (agent_id, agent_name, team, active) VALUES
    (1, 'Marina Costa',    'Suporte N1', TRUE),
    (2, 'Rafael Lima',     'Suporte N1', TRUE),
    (3, 'Camila Duarte',   'Suporte N2', TRUE),
    (4, 'Thiago Moraes',   'Suporte N2', TRUE),
    (5, 'Patrícia Alves',  'Faturamento', TRUE),
    (6, 'Vinícius Rocha',  'Suporte N1', FALSE),
    (7, 'Larissa Freitas', 'Suporte N2', TRUE),
    (8, 'Eduardo Pinto',   'Faturamento', TRUE);

-- ---------------------------------------------------------------------
-- customers (10) — a maioria ativa, um cancelado (caso 8010)
-- ---------------------------------------------------------------------
INSERT INTO customers (customer_id, company_name, plan_status, created_at) VALUES
    (300, 'Cerâmica Vitória Ltda',  'active',    '2025-01-15 09:00:00'),
    (301, 'Mercado Bom Preço',      'active',    '2025-02-20 09:00:00'),
    (302, 'Auto Peças Rondon',      'active',    '2025-03-10 09:00:00'),
    (303, 'Studio Fotográfico Luz', 'active',    '2025-04-05 09:00:00'),
    (304, 'Transportadora Serra',   'cancelled', '2025-01-22 09:00:00'),
    (305, 'Farmácia Vida Plena',    'active',    '2025-05-18 09:00:00'),
    (306, 'Escritório Contábil JR', 'active',    '2025-06-02 09:00:00'),
    (307, 'Pet Shop Amigo Fiel',    'active',    '2025-06-25 09:00:00'),
    (308, 'Papelaria Criativa',     'active',    '2025-07-01 09:00:00'),
    (309, 'Academia Corpo & Cia',   'active',    '2025-07-10 09:00:00');

-- -----------------------------------------------------------------------
-- 8001-8010 — casos canônicos
-- -----------------------------------------------------------------------

-- 8001: caminho feliz — aberto -> em andamento -> resolvido, dentro do SLA
INSERT INTO support_tickets (ticket_id, customer_id, agent_id, subject, priority, status, sla_minutes, created_at, resolved_at, closed_at) VALUES
    (8001, 300, 1, 'Erro ao emitir nota fiscal', 2, 3, 240, '2026-07-10 09:00:00', '2026-07-10 11:30:00', '2026-07-10 12:00:00');
INSERT INTO ticket_events (event_id, ticket_id, event_type, agent_id, created_at) VALUES
    (1, 8001, 'created', NULL, '2026-07-10 09:00:00'),
    (2, 8001, 'assigned', 1, '2026-07-10 09:05:00'),
    (3, 8001, 'status_changed', 1, '2026-07-10 09:30:00'),
    (4, 8001, 'resolved', 1, '2026-07-10 11:30:00'),
    (5, 8001, 'closed', 1, '2026-07-10 12:00:00');

-- 8002: status "resolvido" mas resolved_at NULL — inconsistência de dado
INSERT INTO support_tickets (ticket_id, customer_id, agent_id, subject, priority, status, sla_minutes, created_at, resolved_at, closed_at) VALUES
    (8002, 301, 2, 'Dúvida sobre integração via API', 1, 3, 480, '2026-07-11 10:00:00', NULL, NULL);
INSERT INTO ticket_events (event_id, ticket_id, event_type, agent_id, created_at) VALUES
    (6, 8002, 'created', NULL, '2026-07-11 10:00:00'),
    (7, 8002, 'assigned', 2, '2026-07-11 10:10:00'),
    (8, 8002, 'resolved', 2, '2026-07-11 15:00:00');

-- 8003: SLA estourado (resolved_at - created_at > sla_minutes)
INSERT INTO support_tickets (ticket_id, customer_id, agent_id, subject, priority, status, sla_minutes, created_at, resolved_at, closed_at) VALUES
    (8003, 302, 3, 'Sistema fora do ar em horário de pico', 3, 3, 60, '2026-07-12 08:00:00', '2026-07-12 14:00:00', '2026-07-12 14:30:00');
INSERT INTO ticket_events (event_id, ticket_id, event_type, agent_id, created_at) VALUES
    (9, 8003, 'created', NULL, '2026-07-12 08:00:00'),
    (10, 8003, 'assigned', 3, '2026-07-12 08:20:00'),
    (11, 8003, 'resolved', 3, '2026-07-12 14:00:00'),
    (12, 8003, 'closed', 3, '2026-07-12 14:30:00');

-- 8004: reaberto (evento 'reopened'), mas sem agente atribuído no momento
INSERT INTO support_tickets (ticket_id, customer_id, agent_id, subject, priority, status, sla_minutes, created_at, resolved_at, closed_at) VALUES
    (8004, 303, NULL, 'Relatório mensal com valores errados', 2, 4, 240, '2026-07-13 09:00:00', NULL, NULL);
INSERT INTO ticket_events (event_id, ticket_id, event_type, agent_id, created_at) VALUES
    (13, 8004, 'created', NULL, '2026-07-13 09:00:00'),
    (14, 8004, 'assigned', 4, '2026-07-13 09:15:00'),
    (15, 8004, 'resolved', 4, '2026-07-13 11:00:00'),
    (16, 8004, 'closed', 4, '2026-07-13 11:30:00'),
    (17, 8004, 'reopened', NULL, '2026-07-14 08:00:00');

-- 8005: "em andamento" mas nunca teve evento de atribuição de agente
INSERT INTO support_tickets (ticket_id, customer_id, agent_id, subject, priority, status, sla_minutes, created_at, resolved_at, closed_at) VALUES
    (8005, 304, NULL, 'Cobrança duplicada no cartão', 3, 2, 120, '2026-07-14 13:00:00', NULL, NULL);
INSERT INTO ticket_events (event_id, ticket_id, event_type, agent_id, created_at) VALUES
    (18, 8005, 'created', NULL, '2026-07-14 13:00:00'),
    (19, 8005, 'status_changed', NULL, '2026-07-14 13:10:00');

-- 8006: prioridade com valor sem significado conhecido (99)
INSERT INTO support_tickets (ticket_id, customer_id, agent_id, subject, priority, status, sla_minutes, created_at, resolved_at, closed_at) VALUES
    (8006, 305, 5, 'Solicitação de segunda via de boleto', 99, 1, 480, '2026-07-15 10:00:00', NULL, NULL);
INSERT INTO ticket_events (event_id, ticket_id, event_type, agent_id, created_at) VALUES
    (20, 8006, 'created', NULL, '2026-07-15 10:00:00');

-- 8007: resolvido mas sem NENHUM ticket_events — resolução "fantasma"
INSERT INTO support_tickets (ticket_id, customer_id, agent_id, subject, priority, status, sla_minutes, created_at, resolved_at, closed_at) VALUES
    (8007, 306, 7, 'Exportação de dados incompleta', 2, 3, 240, '2026-07-16 09:00:00', '2026-07-16 10:00:00', '2026-07-16 10:30:00');

-- 8008: ainda aberto, dentro do SLA — caminho normal em andamento
INSERT INTO support_tickets (ticket_id, customer_id, agent_id, subject, priority, status, sla_minutes, created_at, resolved_at, closed_at) VALUES
    (8008, 307, 2, 'Como cadastrar novo usuário', 1, 1, 480, '2026-08-03 09:00:00', NULL, NULL);
INSERT INTO ticket_events (event_id, ticket_id, event_type, agent_id, created_at) VALUES
    (21, 8008, 'created', NULL, '2026-08-03 09:00:00'),
    (22, 8008, 'assigned', 2, '2026-08-03 09:20:00');

-- 8009: fechado (status 5) sem NUNCA ter passado por "resolvido"
INSERT INTO support_tickets (ticket_id, customer_id, agent_id, subject, priority, status, sla_minutes, created_at, resolved_at, closed_at) VALUES
    (8009, 308, 1, 'Ticket aberto por engano', 1, 5, 480, '2026-07-17 15:00:00', NULL, '2026-07-17 15:10:00');
INSERT INTO ticket_events (event_id, ticket_id, event_type, agent_id, created_at) VALUES
    (23, 8009, 'created', NULL, '2026-07-17 15:00:00'),
    (24, 8009, 'closed', 1, '2026-07-17 15:10:00');

-- 8010: cliente com plano CANCELADO (customer 304), mas ticket aberto
-- depois da data de cancelamento — usa o mesmo cliente do caso 8005, que
-- já tem plan_status = 'cancelled' desde a criação; este ticket foi aberto
-- bem depois disso.
INSERT INTO support_tickets (ticket_id, customer_id, agent_id, subject, priority, status, sla_minutes, created_at, resolved_at, closed_at) VALUES
    (8010, 304, NULL, 'Reclamação pós-cancelamento do plano', 2, 1, 240, '2026-07-20 09:00:00', NULL, NULL);
INSERT INTO ticket_events (event_id, ticket_id, event_type, agent_id, created_at) VALUES
    (25, 8010, 'created', NULL, '2026-07-20 09:00:00');

-- -----------------------------------------------------------------------
-- 8011-8040 — volume extra pra explorar filtro por time, prioridade, data
-- e status à vontade
-- -----------------------------------------------------------------------
INSERT INTO support_tickets (ticket_id, customer_id, agent_id, subject, priority, status, sla_minutes, created_at, resolved_at, closed_at) VALUES
    (8011, 300, 1, 'Erro de layout no painel',            1, 3, 480, '2026-06-01 09:00:00', '2026-06-01 12:00:00', '2026-06-01 12:30:00'),
    (8012, 301, 2, 'Lentidão ao carregar relatórios',      2, 3, 240, '2026-06-02 10:00:00', '2026-06-02 12:00:00', '2026-06-02 12:20:00'),
    (8013, 302, 3, 'Solicitação de novo recurso',          1, 1, 480, '2026-06-03 11:00:00', NULL, NULL),
    (8014, 303, 4, 'Erro 500 ao salvar formulário',        3, 3, 60,  '2026-06-04 08:30:00', '2026-06-04 09:00:00', '2026-06-04 09:15:00'),
    (8015, 305, 5, 'Dúvida sobre plano de assinatura',     1, 5, 480, '2026-06-05 14:00:00', '2026-06-05 15:00:00', '2026-06-05 15:30:00'),
    (8016, 306, 7, 'Falha na sincronização de dados',      2, 2, 240, '2026-06-06 09:00:00', NULL, NULL),
    (8017, 307, 6, 'Acesso bloqueado sem motivo aparente',3, 3, 60,  '2026-06-07 10:00:00', '2026-06-07 10:45:00', '2026-06-07 11:00:00'),
    (8018, 308, 8, 'Ajuste de dados cadastrais',           1, 3, 480, '2026-06-08 13:00:00', '2026-06-08 14:00:00', '2026-06-08 14:10:00'),
    (8019, 309, 1, 'Integração com sistema de terceiros',  2, 1, 240, '2026-06-09 09:30:00', NULL, NULL),
    (8020, 300, 2, 'Problema ao anexar arquivos',          1, 3, 480, '2026-06-10 11:00:00', '2026-06-10 12:00:00', '2026-06-10 12:15:00'),
    (8021, 301, 3, 'Cliente não recebe e-mail de confirmação', 2, 2, 240, '2026-06-11 15:00:00', NULL, NULL),
    (8022, 302, 4, 'Divergência de valores no relatório',  3, 3, 60,  '2026-06-12 08:00:00', '2026-06-12 08:50:00', '2026-06-12 09:00:00'),
    (8023, 303, 5, 'Solicitação de exportação em massa',   1, 1, 480, '2026-06-13 10:00:00', NULL, NULL),
    (8024, 305, 7, 'Erro ao trocar senha',                 2, 3, 240, '2026-06-14 09:00:00', '2026-06-14 10:30:00', '2026-06-14 10:40:00'),
    (8025, 306, 8, 'Tela em branco após login',             3, 4, 60,  '2026-06-15 08:00:00', '2026-06-15 08:40:00', NULL),
    (8026, 307, 1, 'Dúvida sobre faturamento proporcional', 1, 5, 480, '2026-06-16 14:00:00', '2026-06-16 15:00:00', '2026-06-16 15:20:00'),
    (8027, 308, 2, 'Falha ao gerar PDF',                    2, 3, 240, '2026-06-17 09:00:00', '2026-06-17 11:00:00', '2026-06-17 11:10:00'),
    (8028, 309, 3, 'Erro de permissão em relatório',        1, 2, 480, '2026-06-18 10:00:00', NULL, NULL),
    (8029, 300, 4, 'Solicitação de treinamento da equipe',  1, 1, 480, '2026-06-19 09:00:00', NULL, NULL),
    (8030, 301, 5, 'Cobrança indevida em fatura',           3, 3, 60,  '2026-06-20 08:00:00', '2026-06-20 08:55:00', '2026-06-20 09:05:00'),
    (8031, 302, 6, 'Ticket duplicado por engano',           1, 5, 480, '2026-06-21 11:00:00', '2026-06-21 11:20:00', '2026-06-21 11:25:00'),
    (8032, 303, 7, 'Erro ao importar planilha',             2, 3, 240, '2026-06-22 09:00:00', '2026-06-22 10:15:00', '2026-06-22 10:20:00'),
    (8033, 305, 8, 'Solicitação de segunda via de contrato', 1, 1, 480, '2026-06-23 13:00:00', NULL, NULL),
    (8034, 306, 1, 'Falha ao aplicar cupom de desconto',    2, 2, 240, '2026-06-24 10:00:00', NULL, NULL),
    (8035, 307, 2, 'Dashboard não atualiza em tempo real',  3, 3, 60,  '2026-06-25 08:00:00', '2026-06-25 08:45:00', '2026-06-25 08:55:00'),
    (8036, 308, 3, 'Solicitação de cancelamento de ticket', 1, 5, 480, '2026-06-26 14:00:00', '2026-06-26 14:10:00', '2026-06-26 14:15:00'),
    (8037, 309, 4, 'Erro ao vincular novo domínio',         2, 3, 240, '2026-06-27 09:00:00', '2026-06-27 10:00:00', '2026-06-27 10:10:00'),
    (8038, 300, 5, 'Dúvida sobre limite de usuários',       1, 1, 480, '2026-06-28 11:00:00', NULL, NULL),
    (8039, 301, 7, 'Falha crítica em produção',             3, 4, 30,  '2026-06-29 07:00:00', '2026-06-29 07:40:00', NULL),
    (8040, 302, 8, 'Solicitação de relatório customizado',  1, 1, 480, '2026-06-30 10:00:00', NULL, NULL);

INSERT INTO ticket_events (event_id, ticket_id, event_type, agent_id, created_at) VALUES
    (26, 8011, 'created', NULL, '2026-06-01 09:00:00'), (27, 8011, 'assigned', 1, '2026-06-01 09:10:00'),
    (28, 8012, 'created', NULL, '2026-06-02 10:00:00'), (29, 8012, 'assigned', 2, '2026-06-02 10:05:00'),
    (30, 8013, 'created', NULL, '2026-06-03 11:00:00'), (31, 8013, 'assigned', 3, '2026-06-03 11:05:00'),
    (32, 8014, 'created', NULL, '2026-06-04 08:30:00'), (33, 8014, 'assigned', 4, '2026-06-04 08:35:00'),
    (34, 8016, 'created', NULL, '2026-06-06 09:00:00'), (35, 8016, 'assigned', 7, '2026-06-06 09:05:00'),
    (36, 8019, 'created', NULL, '2026-06-09 09:30:00'), (37, 8019, 'assigned', 1, '2026-06-09 09:40:00'),
    (38, 8021, 'created', NULL, '2026-06-11 15:00:00'), (39, 8021, 'assigned', 3, '2026-06-11 15:10:00'),
    (40, 8023, 'created', NULL, '2026-06-13 10:00:00'), (41, 8023, 'assigned', 5, '2026-06-13 10:10:00'),
    (42, 8025, 'created', NULL, '2026-06-15 08:00:00'), (43, 8025, 'reopened', 8, '2026-06-15 09:00:00'),
    (44, 8028, 'created', NULL, '2026-06-18 10:00:00'), (45, 8028, 'assigned', 3, '2026-06-18 10:10:00'),
    (46, 8029, 'created', NULL, '2026-06-19 09:00:00'), (47, 8029, 'assigned', 4, '2026-06-19 09:10:00'),
    (48, 8034, 'created', NULL, '2026-06-24 10:00:00'), (49, 8034, 'assigned', 1, '2026-06-24 10:10:00'),
    (50, 8038, 'created', NULL, '2026-06-28 11:00:00'), (51, 8038, 'assigned', 5, '2026-06-28 11:10:00'),
    (52, 8039, 'created', NULL, '2026-06-29 07:00:00'), (53, 8039, 'reopened', 7, '2026-06-29 07:45:00'),
    (54, 8040, 'created', NULL, '2026-06-30 10:00:00'), (55, 8040, 'assigned', 8, '2026-06-30 10:10:00');
