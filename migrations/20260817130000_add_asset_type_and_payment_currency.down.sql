ALTER TABLE movements DROP CONSTRAINT IF EXISTS movements_paid_amount_positive;
ALTER TABLE movements DROP COLUMN IF EXISTS paid_currency_id;
ALTER TABLE movements DROP COLUMN IF EXISTS paid_amount;
ALTER TABLE assets DROP COLUMN IF EXISTS asset_type;
