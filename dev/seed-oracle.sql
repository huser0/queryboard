-- Fixture de teste local para o queryboard — domínio Oracle, diferente do
-- Postgres (oferta/pedido) e do MySQL (helpdesk): RH e folha de pagamento.
-- departamento -> funcionário -> apontamento de horas -> folha gerada.
--
-- Uso: ver "Guia de teste local" no README.md. Sintaxe assume Oracle 23ai
-- (`DROP TABLE IF EXISTS` é suportado a partir do 23c) — mesma versão do
-- container `gvenzl/oracle-free:23-slim` usado no docker-compose.
--
-- payroll_id 9001-9010 são os casos "canônicos" documentados no README —
-- não mude o significado deles. 9011 em diante é só volume/variedade
-- extra pra explorar filtros, datas, status etc.
--
-- O entrypoint do container roda scripts de init via `sqlplus / as sysdba`
-- (bequeath, autenticação de SO) — isso conecta como SYS no CDB$ROOT, não
-- no app user/PDB configurado por APP_USER. Sem trocar container+schema
-- explicitamente, todas as tabelas seriam criadas embaixo de SYS na raiz,
-- inacessíveis para a connection `queryboard@FREEPDB1` que o app usa
-- (confirmado rodando o seed original sem isso: `select table_name from
-- user_tables` como queryboard devolvia zero linhas).
ALTER SESSION SET CONTAINER = FREEPDB1;
ALTER SESSION SET CURRENT_SCHEMA = QUERYBOARD;

DROP TABLE IF EXISTS payroll_runs;
DROP TABLE IF EXISTS timesheets;
DROP TABLE IF EXISTS employees;
DROP TABLE IF EXISTS departments;

CREATE TABLE departments (
    department_id          NUMBER(6) PRIMARY KEY,
    department_name        VARCHAR2(120) NOT NULL,
    manager_employee_id    NUMBER(6) -- NULL = sem gestor definido
);

CREATE TABLE employees (
    employee_id       NUMBER(6) PRIMARY KEY,
    department_id     NUMBER(6) NOT NULL REFERENCES departments(department_id),
    full_name         VARCHAR2(160) NOT NULL,
    hire_date         DATE NOT NULL,
    termination_date  DATE, -- NULL = ainda ativo
    salary            NUMBER(12,2) NOT NULL
);

CREATE TABLE timesheets (
    timesheet_id  NUMBER(8) PRIMARY KEY,
    employee_id   NUMBER(6) NOT NULL REFERENCES employees(employee_id),
    work_date     DATE NOT NULL,
    hours_worked  NUMBER(4,2) NOT NULL,
    approved      CHAR(1) NOT NULL, -- 'Y' | 'N'
    approved_by   NUMBER(6) REFERENCES employees(employee_id)
);

CREATE TABLE payroll_runs (
    payroll_id     NUMBER(8) PRIMARY KEY,
    employee_id    NUMBER(6) NOT NULL REFERENCES employees(employee_id),
    period_start   DATE NOT NULL,
    period_end     DATE NOT NULL,
    gross_amount   NUMBER(12,2) NOT NULL,
    status         NUMBER(3) NOT NULL, -- 1=rascunho 2=aprovado 3=pago 99=desconhecido
    generated_at   TIMESTAMP
);

-- ---------------------------------------------------------------------
-- departments (6) — um deles de propósito sem gestor (caso 9005)
-- ---------------------------------------------------------------------
INSERT INTO departments (department_id, department_name, manager_employee_id) VALUES (90, 'Engenharia', 901);
INSERT INTO departments (department_id, department_name, manager_employee_id) VALUES (91, 'Vendas', 905);
INSERT INTO departments (department_id, department_name, manager_employee_id) VALUES (92, 'Financeiro', NULL);
INSERT INTO departments (department_id, department_name, manager_employee_id) VALUES (93, 'Recursos Humanos', 910);
INSERT INTO departments (department_id, department_name, manager_employee_id) VALUES (94, 'Suporte ao Cliente', 912);
INSERT INTO departments (department_id, department_name, manager_employee_id) VALUES (95, 'Marketing', 915);

-- ---------------------------------------------------------------------
-- employees (16) — a maioria ativa, dois desligados (900+, usados no
-- caso 9004)
-- ---------------------------------------------------------------------
INSERT INTO employees (employee_id, department_id, full_name, hire_date, termination_date, salary) VALUES (901, 90, 'Renata Souza',      DATE '2022-03-01', NULL, 12500.00);
INSERT INTO employees (employee_id, department_id, full_name, hire_date, termination_date, salary) VALUES (902, 90, 'Felipe Andrade',     DATE '2023-01-10', NULL, 9800.00);
INSERT INTO employees (employee_id, department_id, full_name, hire_date, termination_date, salary) VALUES (903, 90, 'Juliana Ramos',      DATE '2023-06-15', NULL, 9800.00);
INSERT INTO employees (employee_id, department_id, full_name, hire_date, termination_date, salary) VALUES (904, 90, 'Marcelo Teixeira',   DATE '2021-11-20', DATE '2026-06-30', 10200.00);
INSERT INTO employees (employee_id, department_id, full_name, hire_date, termination_date, salary) VALUES (905, 91, 'Beatriz Cardoso',    DATE '2020-05-05', NULL, 11000.00);
INSERT INTO employees (employee_id, department_id, full_name, hire_date, termination_date, salary) VALUES (906, 91, 'Gustavo Pereira',    DATE '2022-09-12', NULL, 7500.00);
INSERT INTO employees (employee_id, department_id, full_name, hire_date, termination_date, salary) VALUES (907, 91, 'Aline Barbosa',      DATE '2024-02-01', NULL, 7500.00);
INSERT INTO employees (employee_id, department_id, full_name, hire_date, termination_date, salary) VALUES (908, 92, 'Rodrigo Martins',    DATE '2019-08-18', NULL, 9200.00);
INSERT INTO employees (employee_id, department_id, full_name, hire_date, termination_date, salary) VALUES (909, 92, 'Camila Ferreira',    DATE '2023-03-22', NULL, 8100.00);
INSERT INTO employees (employee_id, department_id, full_name, hire_date, termination_date, salary) VALUES (910, 93, 'Diego Santos',       DATE '2021-01-11', NULL, 10500.00);
INSERT INTO employees (employee_id, department_id, full_name, hire_date, termination_date, salary) VALUES (911, 93, 'Vanessa Lima',       DATE '2024-05-06', NULL, 7200.00);
INSERT INTO employees (employee_id, department_id, full_name, hire_date, termination_date, salary) VALUES (912, 94, 'Bruno Cavalcanti',   DATE '2022-07-19', NULL, 8600.00);
INSERT INTO employees (employee_id, department_id, full_name, hire_date, termination_date, salary) VALUES (913, 94, 'Tatiane Nogueira',   DATE '2023-10-02', NULL, 6800.00);
INSERT INTO employees (employee_id, department_id, full_name, hire_date, termination_date, salary) VALUES (914, 94, 'Leonardo Vieira',    DATE '2020-12-14', DATE '2026-05-15', 7000.00);
INSERT INTO employees (employee_id, department_id, full_name, hire_date, termination_date, salary) VALUES (915, 95, 'Priscila Monteiro',  DATE '2021-04-09', NULL, 9600.00);
INSERT INTO employees (employee_id, department_id, full_name, hire_date, termination_date, salary) VALUES (916, 95, 'André Nascimento',   DATE '2024-08-25', NULL, 6500.00);

-- -----------------------------------------------------------------------
-- 9001-9010 — casos canônicos (payroll_id igual ao número do caso)
-- -----------------------------------------------------------------------

-- 9001: caminho feliz — timesheet completo e aprovado, folha bate com as
-- horas lançadas (22 dias úteis x 8h = 176h; salário/mês integral, pago)
INSERT INTO timesheets (timesheet_id, employee_id, work_date, hours_worked, approved, approved_by) VALUES (90001, 902, DATE '2026-07-01', 8.00, 'Y', 901);
INSERT INTO timesheets (timesheet_id, employee_id, work_date, hours_worked, approved, approved_by) VALUES (90002, 902, DATE '2026-07-02', 8.00, 'Y', 901);
INSERT INTO payroll_runs (payroll_id, employee_id, period_start, period_end, gross_amount, status, generated_at) VALUES (9001, 902, DATE '2026-07-01', DATE '2026-07-31', 9800.00, 3, TIMESTAMP '2026-08-01 09:00:00');

-- 9002: horas lançadas mas NÃO aprovadas (approved = 'N'), e mesmo assim
-- a folha já foi gerada — inconsistência de processo
INSERT INTO timesheets (timesheet_id, employee_id, work_date, hours_worked, approved, approved_by) VALUES (90003, 903, DATE '2026-07-01', 8.00, 'N', NULL);
INSERT INTO timesheets (timesheet_id, employee_id, work_date, hours_worked, approved, approved_by) VALUES (90004, 903, DATE '2026-07-02', 8.00, 'N', NULL);
INSERT INTO payroll_runs (payroll_id, employee_id, period_start, period_end, gross_amount, status, generated_at) VALUES (9002, 903, DATE '2026-07-01', DATE '2026-07-31', 9800.00, 3, TIMESTAMP '2026-08-01 09:00:00');

-- 9003: gross_amount DIVERGENTE do salário do funcionário (folha calculada
-- errada, deveria ser 7500.00)
INSERT INTO timesheets (timesheet_id, employee_id, work_date, hours_worked, approved, approved_by) VALUES (90005, 906, DATE '2026-07-01', 8.00, 'Y', 905);
INSERT INTO payroll_runs (payroll_id, employee_id, period_start, period_end, gross_amount, status, generated_at) VALUES (9003, 906, DATE '2026-07-01', DATE '2026-07-31', 6900.00, 3, TIMESTAMP '2026-08-01 09:00:00');

-- 9004: funcionário DESLIGADO (termination_date 2026-06-30), mas folha
-- gerada pra período depois do desligamento
INSERT INTO payroll_runs (payroll_id, employee_id, period_start, period_end, gross_amount, status, generated_at) VALUES (9004, 904, DATE '2026-07-01', DATE '2026-07-31', 10200.00, 3, TIMESTAMP '2026-08-01 09:00:00');

-- 9005: employee_id 909 é do departamento 92 (Financeiro), que não tem
-- manager_employee_id definido — inconsistência de cadastro de
-- departamento, não de folha; incluído aqui pra dar um caso pra explorar
-- via JOIN departments/employees.
INSERT INTO timesheets (timesheet_id, employee_id, work_date, hours_worked, approved, approved_by) VALUES (90006, 909, DATE '2026-07-01', 8.00, 'Y', 908);
INSERT INTO payroll_runs (payroll_id, employee_id, period_start, period_end, gross_amount, status, generated_at) VALUES (9005, 909, DATE '2026-07-01', DATE '2026-07-31', 8100.00, 3, TIMESTAMP '2026-08-01 09:00:00');

-- 9006: status da folha com valor sem significado conhecido (99)
INSERT INTO timesheets (timesheet_id, employee_id, work_date, hours_worked, approved, approved_by) VALUES (90007, 911, DATE '2026-07-01', 8.00, 'Y', 910);
INSERT INTO payroll_runs (payroll_id, employee_id, period_start, period_end, gross_amount, status, generated_at) VALUES (9006, 911, DATE '2026-07-01', DATE '2026-07-31', 7200.00, 99, TIMESTAMP '2026-08-01 09:00:00');

-- 9007: status "pago" (3) mas SEM nenhum timesheet no período — folha
-- "fantasma"
INSERT INTO payroll_runs (payroll_id, employee_id, period_start, period_end, gross_amount, status, generated_at) VALUES (9007, 913, DATE '2026-07-01', DATE '2026-07-31', 6800.00, 3, TIMESTAMP '2026-08-01 09:00:00');

-- 9008: timesheet normal lançado, ainda SEM folha gerada — caminho normal
-- em andamento (mês corrente)
INSERT INTO timesheets (timesheet_id, employee_id, work_date, hours_worked, approved, approved_by) VALUES (90008, 907, DATE '2026-08-01', 8.00, 'Y', 905);
INSERT INTO timesheets (timesheet_id, employee_id, work_date, hours_worked, approved, approved_by) VALUES (90009, 907, DATE '2026-08-02', 8.00, 'Y', 905);

-- 9009: folha "paga" (3) sem nunca ter passado por "aprovado" (2) — pulou
-- etapa do processo (o app não guarda histórico de status aqui, mas dá
-- pra cruzar com a ausência de timesheets aprovados no período)
INSERT INTO timesheets (timesheet_id, employee_id, work_date, hours_worked, approved, approved_by) VALUES (90010, 915, DATE '2026-07-01', 8.00, 'N', NULL);
INSERT INTO payroll_runs (payroll_id, employee_id, period_start, period_end, gross_amount, status, generated_at) VALUES (9009, 915, DATE '2026-07-01', DATE '2026-07-31', 9600.00, 3, TIMESTAMP '2026-08-01 09:00:00');

-- 9010: funcionário ativo, folha em rascunho (1), mas SEM nenhum
-- timesheet lançado no período do payroll
INSERT INTO payroll_runs (payroll_id, employee_id, period_start, period_end, gross_amount, status, generated_at) VALUES (9010, 916, DATE '2026-08-01', DATE '2026-08-31', 6500.00, 1, NULL);

-- -----------------------------------------------------------------------
-- 9011-9040 — volume extra pra explorar filtro por departamento, status,
-- data e valor à vontade
-- -----------------------------------------------------------------------
INSERT INTO timesheets (timesheet_id, employee_id, work_date, hours_worked, approved, approved_by) VALUES (90011, 901, DATE '2026-06-01', 8.00, 'Y', 901);
INSERT INTO timesheets (timesheet_id, employee_id, work_date, hours_worked, approved, approved_by) VALUES (90012, 902, DATE '2026-06-01', 8.00, 'Y', 901);
INSERT INTO timesheets (timesheet_id, employee_id, work_date, hours_worked, approved, approved_by) VALUES (90013, 905, DATE '2026-06-01', 8.00, 'Y', 905);
INSERT INTO timesheets (timesheet_id, employee_id, work_date, hours_worked, approved, approved_by) VALUES (90014, 906, DATE '2026-06-01', 8.00, 'Y', 905);
INSERT INTO timesheets (timesheet_id, employee_id, work_date, hours_worked, approved, approved_by) VALUES (90015, 908, DATE '2026-06-01', 8.00, 'Y', 908);
INSERT INTO timesheets (timesheet_id, employee_id, work_date, hours_worked, approved, approved_by) VALUES (90016, 910, DATE '2026-06-01', 8.00, 'Y', 910);
INSERT INTO timesheets (timesheet_id, employee_id, work_date, hours_worked, approved, approved_by) VALUES (90017, 912, DATE '2026-06-01', 8.00, 'Y', 912);
INSERT INTO timesheets (timesheet_id, employee_id, work_date, hours_worked, approved, approved_by) VALUES (90018, 913, DATE '2026-06-01', 8.00, 'Y', 912);
INSERT INTO timesheets (timesheet_id, employee_id, work_date, hours_worked, approved, approved_by) VALUES (90019, 915, DATE '2026-06-01', 8.00, 'Y', 915);
INSERT INTO timesheets (timesheet_id, employee_id, work_date, hours_worked, approved, approved_by) VALUES (90020, 916, DATE '2026-06-01', 8.00, 'Y', 915);

INSERT INTO payroll_runs (payroll_id, employee_id, period_start, period_end, gross_amount, status, generated_at) VALUES (9011, 901, DATE '2026-06-01', DATE '2026-06-30', 12500.00, 3, TIMESTAMP '2026-07-01 09:00:00');
INSERT INTO payroll_runs (payroll_id, employee_id, period_start, period_end, gross_amount, status, generated_at) VALUES (9012, 902, DATE '2026-06-01', DATE '2026-06-30', 9800.00,  3, TIMESTAMP '2026-07-01 09:00:00');
INSERT INTO payroll_runs (payroll_id, employee_id, period_start, period_end, gross_amount, status, generated_at) VALUES (9013, 905, DATE '2026-06-01', DATE '2026-06-30', 11000.00, 3, TIMESTAMP '2026-07-01 09:00:00');
INSERT INTO payroll_runs (payroll_id, employee_id, period_start, period_end, gross_amount, status, generated_at) VALUES (9014, 906, DATE '2026-06-01', DATE '2026-06-30', 7500.00,  3, TIMESTAMP '2026-07-01 09:00:00');
INSERT INTO payroll_runs (payroll_id, employee_id, period_start, period_end, gross_amount, status, generated_at) VALUES (9015, 908, DATE '2026-06-01', DATE '2026-06-30', 9200.00,  3, TIMESTAMP '2026-07-01 09:00:00');
INSERT INTO payroll_runs (payroll_id, employee_id, period_start, period_end, gross_amount, status, generated_at) VALUES (9016, 910, DATE '2026-06-01', DATE '2026-06-30', 10500.00, 3, TIMESTAMP '2026-07-01 09:00:00');
INSERT INTO payroll_runs (payroll_id, employee_id, period_start, period_end, gross_amount, status, generated_at) VALUES (9017, 912, DATE '2026-06-01', DATE '2026-06-30', 8600.00,  3, TIMESTAMP '2026-07-01 09:00:00');
INSERT INTO payroll_runs (payroll_id, employee_id, period_start, period_end, gross_amount, status, generated_at) VALUES (9018, 913, DATE '2026-06-01', DATE '2026-06-30', 6800.00,  3, TIMESTAMP '2026-07-01 09:00:00');
INSERT INTO payroll_runs (payroll_id, employee_id, period_start, period_end, gross_amount, status, generated_at) VALUES (9019, 915, DATE '2026-06-01', DATE '2026-06-30', 9600.00,  3, TIMESTAMP '2026-07-01 09:00:00');
INSERT INTO payroll_runs (payroll_id, employee_id, period_start, period_end, gross_amount, status, generated_at) VALUES (9020, 916, DATE '2026-06-01', DATE '2026-06-30', 6500.00,  3, TIMESTAMP '2026-07-01 09:00:00');
INSERT INTO payroll_runs (payroll_id, employee_id, period_start, period_end, gross_amount, status, generated_at) VALUES (9021, 901, DATE '2026-07-01', DATE '2026-07-31', 12500.00, 2, TIMESTAMP '2026-08-01 09:00:00');
INSERT INTO payroll_runs (payroll_id, employee_id, period_start, period_end, gross_amount, status, generated_at) VALUES (9022, 905, DATE '2026-07-01', DATE '2026-07-31', 11000.00, 2, TIMESTAMP '2026-08-01 09:00:00');
INSERT INTO payroll_runs (payroll_id, employee_id, period_start, period_end, gross_amount, status, generated_at) VALUES (9023, 908, DATE '2026-07-01', DATE '2026-07-31', 9200.00,  1, NULL);
INSERT INTO payroll_runs (payroll_id, employee_id, period_start, period_end, gross_amount, status, generated_at) VALUES (9024, 910, DATE '2026-07-01', DATE '2026-07-31', 10500.00, 1, NULL);
INSERT INTO payroll_runs (payroll_id, employee_id, period_start, period_end, gross_amount, status, generated_at) VALUES (9025, 912, DATE '2026-07-01', DATE '2026-07-31', 8600.00,  2, TIMESTAMP '2026-08-01 09:00:00');
INSERT INTO payroll_runs (payroll_id, employee_id, period_start, period_end, gross_amount, status, generated_at) VALUES (9026, 913, DATE '2026-07-01', DATE '2026-07-31', 6800.00,  2, TIMESTAMP '2026-08-01 09:00:00');
INSERT INTO payroll_runs (payroll_id, employee_id, period_start, period_end, gross_amount, status, generated_at) VALUES (9027, 916, DATE '2026-07-01', DATE '2026-07-31', 6500.00,  3, TIMESTAMP '2026-08-01 09:00:00');
INSERT INTO payroll_runs (payroll_id, employee_id, period_start, period_end, gross_amount, status, generated_at) VALUES (9028, 909, DATE '2026-06-01', DATE '2026-06-30', 8100.00,  3, TIMESTAMP '2026-07-01 09:00:00');
INSERT INTO payroll_runs (payroll_id, employee_id, period_start, period_end, gross_amount, status, generated_at) VALUES (9029, 911, DATE '2026-06-01', DATE '2026-06-30', 7200.00,  3, TIMESTAMP '2026-07-01 09:00:00');
INSERT INTO payroll_runs (payroll_id, employee_id, period_start, period_end, gross_amount, status, generated_at) VALUES (9030, 907, DATE '2026-06-01', DATE '2026-06-30', 7500.00,  3, TIMESTAMP '2026-07-01 09:00:00');
INSERT INTO payroll_runs (payroll_id, employee_id, period_start, period_end, gross_amount, status, generated_at) VALUES (9031, 903, DATE '2026-06-01', DATE '2026-06-30', 9800.00,  3, TIMESTAMP '2026-07-01 09:00:00');
INSERT INTO payroll_runs (payroll_id, employee_id, period_start, period_end, gross_amount, status, generated_at) VALUES (9032, 909, DATE '2026-07-01', DATE '2026-07-31', 8100.00,  2, TIMESTAMP '2026-08-01 09:00:00');
INSERT INTO payroll_runs (payroll_id, employee_id, period_start, period_end, gross_amount, status, generated_at) VALUES (9033, 911, DATE '2026-07-01', DATE '2026-07-31', 7200.00,  2, TIMESTAMP '2026-08-01 09:00:00');
INSERT INTO payroll_runs (payroll_id, employee_id, period_start, period_end, gross_amount, status, generated_at) VALUES (9034, 907, DATE '2026-07-01', DATE '2026-07-31', 7500.00,  1, NULL);
INSERT INTO payroll_runs (payroll_id, employee_id, period_start, period_end, gross_amount, status, generated_at) VALUES (9035, 903, DATE '2026-07-01', DATE '2026-07-31', 9800.00,  1, NULL);
INSERT INTO payroll_runs (payroll_id, employee_id, period_start, period_end, gross_amount, status, generated_at) VALUES (9036, 902, DATE '2026-05-01', DATE '2026-05-31', 9800.00,  3, TIMESTAMP '2026-06-01 09:00:00');
INSERT INTO payroll_runs (payroll_id, employee_id, period_start, period_end, gross_amount, status, generated_at) VALUES (9037, 905, DATE '2026-05-01', DATE '2026-05-31', 11000.00, 3, TIMESTAMP '2026-06-01 09:00:00');
INSERT INTO payroll_runs (payroll_id, employee_id, period_start, period_end, gross_amount, status, generated_at) VALUES (9038, 908, DATE '2026-05-01', DATE '2026-05-31', 9200.00,  3, TIMESTAMP '2026-06-01 09:00:00');
INSERT INTO payroll_runs (payroll_id, employee_id, period_start, period_end, gross_amount, status, generated_at) VALUES (9039, 910, DATE '2026-05-01', DATE '2026-05-31', 10500.00, 3, TIMESTAMP '2026-06-01 09:00:00');
INSERT INTO payroll_runs (payroll_id, employee_id, period_start, period_end, gross_amount, status, generated_at) VALUES (9040, 913, DATE '2026-05-01', DATE '2026-05-31', 6800.00,  3, TIMESTAMP '2026-06-01 09:00:00');
