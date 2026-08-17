CREATE TABLE IF NOT EXISTS movements (
 id BIGSERIAL PRIMARY KEY NOT NULL,
 user_id BIGINT NOT NULL REFERENCES users (id),
 asset_id BIGINT NOT NULL REFERENCES assets (id),
 kind TEXT NOT NULL CHECK (kind IN ('buy', 'sell')),
 quantity DOUBLE PRECISION NOT NULL CHECK (quantity > 0),
 unit_price DOUBLE PRECISION NOT NULL,
 created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS movements_user_asset_idx ON movements (user_id, asset_id);
