-- Exemplos de SELECT pra colar direto num bloco "SQL ad-hoc" do Painel e
-- testar o queryboard contra os dados de dev/seed-postgres.sql.
--
-- Os que têm parâmetro nomeado (:algo) fazem o campo de parâmetro aparecer
-- sozinho acima do bloco — não precisa editar a SQL pra trocar o valor.

-- 1. products — catálogo simples, com filtro booleano
SELECT product_id, product_name, category, base_price, active
FROM products
WHERE active = true
ORDER BY category, product_name;

-- 2. stores — todas as lojas por região
SELECT store_id, store_name, region, city
FROM stores
ORDER BY region, store_name;

-- 3. offers + products + stores — investigação de uma oferta específica
--    (use 5001 = caminho feliz, 5002 = sem envio, 5003 = preço divergente)
SELECT o.offer_id, o.offer_status, o.start_date, p.product_name, s.store_name
FROM offers o
JOIN products p ON p.product_id = o.product_id
JOIN stores s ON s.store_id = o.store_id
WHERE o.offer_id = :offer_id;

-- 4. offers sem envio registrado — reproduz o caso canônico 5002 em lote
SELECT o.offer_id, o.offer_status, o.start_date, p.product_name, s.store_name
FROM offers o
JOIN products p ON p.product_id = o.product_id
JOIN stores s ON s.store_id = o.store_id
LEFT JOIN product_shipments ps
  ON ps.product_id = o.product_id AND ps.store_id = o.store_id
WHERE o.offer_status = 5 AND ps.shipment_id IS NULL
ORDER BY o.offer_id;

-- 5. offer_store_product vs. base_price — reproduz o caso canônico 5003
--    (preço praticado divergente do preço base do produto)
SELECT osp.offer_id, p.product_name, p.base_price, osp.price AS preco_praticado
FROM offer_store_product osp
JOIN products p ON p.product_id = osp.product_id
WHERE osp.price <> p.base_price
ORDER BY osp.offer_id;

-- 6. customers + orders — pedidos de um cliente, mais recentes primeiro
SELECT c.full_name, c.segment, o.order_id, o.order_status, o.order_date, o.total_amount
FROM orders o
JOIN customers c ON c.customer_id = o.customer_id
WHERE c.customer_id = :customer_id
ORDER BY o.order_date DESC;

-- 7. orders + payments — valor pago divergente do total do pedido
--    (reproduz o caso canônico 6002)
SELECT o.order_id, o.total_amount, pay.amount AS valor_pago, pay.payment_status
FROM orders o
JOIN payments pay ON pay.order_id = o.order_id
WHERE pay.amount <> o.total_amount;

-- 8. orders "enviado" sem registro em order_shipments
--    (reproduz o caso canônico 6003)
SELECT o.order_id, o.order_status, o.order_date, c.full_name
FROM orders o
JOIN customers c ON c.customer_id = o.customer_id
LEFT JOIN order_shipments os ON os.order_id = o.order_id
WHERE o.order_status = 4 AND os.order_shipment_id IS NULL;

-- 9. orders cancelados com pagamento ainda aprovado (não estornado)
--    (reproduz o caso canônico 6006)
SELECT o.order_id, o.order_status, pay.payment_status, pay.amount
FROM orders o
JOIN payments pay ON pay.order_id = o.order_id
WHERE o.order_status = 9 AND pay.payment_status = 2;

-- 10. order_items — receita agregada por produto (agregação simples, sem parâmetro)
SELECT p.product_name, SUM(oi.quantity) AS unidades_vendidas, SUM(oi.quantity * oi.unit_price) AS receita
FROM order_items oi
JOIN products p ON p.product_id = oi.product_id
GROUP BY p.product_name
ORDER BY receita DESC;
