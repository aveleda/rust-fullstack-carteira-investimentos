ALTER TABLE assets
 ADD COLUMN asset_type TEXT NOT NULL DEFAULT 'crypto' CHECK (asset_type IN ('crypto', 'fiat'));

ALTER TABLE assets ALTER COLUMN asset_type DROP DEFAULT;

INSERT INTO assets (name, unit_value, asset_type) VALUES
 ('Real', 1, 'fiat'),
 ('Dolar Americano', 5.2, 'fiat'),
 ('Euro', 5.6, 'fiat')
ON CONFLICT (name) DO NOTHING;

ALTER TABLE movements
 ADD COLUMN paid_amount DOUBLE PRECISION,
 ADD COLUMN paid_currency_id BIGINT REFERENCES assets (id);

-- Movimentações registradas antes desta migration não têm moeda de
-- pagamento explícita; assume-se que o preço já estava em reais.
UPDATE movements
SET paid_amount = quantity * unit_price,
    paid_currency_id = (SELECT id FROM assets WHERE name = 'Real')
WHERE paid_amount IS NULL;

ALTER TABLE movements
 ALTER COLUMN paid_amount SET NOT NULL,
 ALTER COLUMN paid_currency_id SET NOT NULL;

ALTER TABLE movements ADD CONSTRAINT movements_paid_amount_positive CHECK (paid_amount > 0);
