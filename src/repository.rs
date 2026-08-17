use std::convert::Infallible;

use axum::extract::FromRequestParts;
use sqlx::PgPool;

use crate::{
    app::AppState,
    models::{Asset, Holding, Movement, UserRecord},
};

pub struct Repository {
    db: PgPool,
}

pub struct NewMovement<'a> {
    pub asset_id: i64,
    pub kind: &'a str,
    pub quantity: f64,
    pub unit_price: f64,
    pub paid_amount: f64,
    pub paid_currency_id: i64,
}

impl Repository {
    pub async fn list_assets(&self) -> sqlx::Result<Vec<Asset>> {
        sqlx::query_as!(
            Asset,
            "SELECT id, name, unit_value, asset_type
             FROM assets
             ORDER BY name;"
        )
        .fetch_all(&self.db)
        .await
    }

    pub async fn create_asset(
        &self,
        name: String,
        unit_value: f64,
        asset_type: String,
    ) -> sqlx::Result<Asset> {
        sqlx::query_as!(
            Asset,
            "INSERT INTO assets (name, unit_value, asset_type)
             VALUES ($1, $2, $3)
             RETURNING id, name, unit_value, asset_type;",
            name,
            unit_value,
            asset_type
        )
        .fetch_one(&self.db)
        .await
    }

    pub async fn update_asset(
        &self,
        asset_id: i64,
        name: Option<String>,
        unit_value: Option<f64>,
    ) -> sqlx::Result<Option<Asset>> {
        sqlx::query_as!(
            Asset,
            "UPDATE assets
             SET name=COALESCE($2, name),
                 unit_value=COALESCE($3, unit_value)
             WHERE id=$1
             RETURNING id, name, unit_value, asset_type;",
            asset_id,
            name,
            unit_value
        )
        .fetch_optional(&self.db)
        .await
    }

    pub async fn add_user(&self, username: &str, password_hash: &str) -> sqlx::Result<UserRecord> {
        sqlx::query_as!(
            UserRecord,
            "INSERT INTO users (username, password_hash)
             VALUES ($1, $2)
             RETURNING id, username, password_hash;",
            username,
            password_hash,
        )
        .fetch_one(&self.db)
        .await
    }

    pub async fn get_user_by_name(&self, username: &str) -> sqlx::Result<Option<UserRecord>> {
        sqlx::query_as!(
            UserRecord,
            "SELECT id, username, password_hash
             FROM users
             WHERE username = $1;",
            username
        )
        .fetch_optional(&self.db)
        .await
    }

    pub async fn get_asset(&self, asset_id: i64) -> sqlx::Result<Option<Asset>> {
        sqlx::query_as!(
            Asset,
            "SELECT id, name, unit_value, asset_type
             FROM assets
             WHERE id = $1;",
            asset_id
        )
        .fetch_optional(&self.db)
        .await
    }

    pub async fn list_user_holdings(&self, user_id: i64) -> sqlx::Result<Vec<Holding>> {
        sqlx::query_as!(
            Holding,
            r#"SELECT a.id AS asset_id, a.name, a.unit_value,
                      SUM(CASE WHEN m.kind = 'buy' THEN m.quantity ELSE -m.quantity END) AS "quantity!: f64",
                      (SUM(CASE WHEN m.kind = 'buy' THEN m.quantity * m.unit_price ELSE 0 END)
                       / SUM(CASE WHEN m.kind = 'buy' THEN m.quantity ELSE 0 END)) AS "avg_unit_price!: f64"
               FROM movements m
               JOIN assets a ON a.id = m.asset_id
               WHERE m.user_id = $1
               GROUP BY a.id, a.name, a.unit_value
               HAVING SUM(CASE WHEN m.kind = 'buy' THEN m.quantity ELSE -m.quantity END) > 0
               ORDER BY a.name;"#,
            user_id
        )
        .fetch_all(&self.db)
        .await
    }

    pub async fn get_holding_quantity(&self, user_id: i64, asset_id: i64) -> sqlx::Result<f64> {
        let row = sqlx::query!(
            r#"SELECT COALESCE(SUM(CASE WHEN kind = 'buy' THEN quantity ELSE -quantity END), 0) AS "quantity!: f64"
               FROM movements
               WHERE user_id = $1 AND asset_id = $2;"#,
            user_id,
            asset_id
        )
        .fetch_one(&self.db)
        .await?;

        Ok(row.quantity)
    }

    pub async fn list_movements(&self, user_id: i64, asset_id: i64) -> sqlx::Result<Vec<Movement>> {
        sqlx::query_as!(
            Movement,
            "SELECT m.id, m.kind, m.quantity, m.unit_price, m.paid_amount,
                    c.name AS paid_currency_name, m.created_at
             FROM movements m
             JOIN assets c ON c.id = m.paid_currency_id
             WHERE m.user_id = $1 AND m.asset_id = $2
             ORDER BY m.created_at DESC;",
            user_id,
            asset_id
        )
        .fetch_all(&self.db)
        .await
    }

    async fn insert_movement<'c, E>(
        executor: E,
        user_id: i64,
        movement: NewMovement<'_>,
    ) -> sqlx::Result<Movement>
    where
        E: sqlx::PgExecutor<'c>,
    {
        sqlx::query_as!(
            Movement,
            r#"WITH inserted AS (
                   INSERT INTO movements (user_id, asset_id, kind, quantity, unit_price, paid_amount, paid_currency_id)
                   VALUES ($1, $2, $3, $4, $5, $6, $7)
                   RETURNING id, kind, quantity, unit_price, paid_amount, paid_currency_id, created_at
               )
               SELECT inserted.id, inserted.kind, inserted.quantity, inserted.unit_price,
                      inserted.paid_amount, c.name AS paid_currency_name, inserted.created_at
               FROM inserted
               JOIN assets c ON c.id = inserted.paid_currency_id;"#,
            user_id,
            movement.asset_id,
            movement.kind,
            movement.quantity,
            movement.unit_price,
            movement.paid_amount,
            movement.paid_currency_id
        )
        .fetch_one(executor)
        .await
    }

    /// Registra uma troca completa: o movimento do ativo negociado
    /// (`primary`) e, atomicamente, o movimento inverso na moeda usada
    /// na troca (`counter`) — é assim que vender uma moeda faz a moeda
    /// recebida aparecer na carteira, e vice-versa na compra.
    pub async fn create_trade(
        &self,
        user_id: i64,
        primary: NewMovement<'_>,
        counter: NewMovement<'_>,
    ) -> sqlx::Result<(Movement, Movement)> {
        let mut tx = self.db.begin().await?;

        let primary_movement = Self::insert_movement(&mut *tx, user_id, primary).await?;
        let counter_movement = Self::insert_movement(&mut *tx, user_id, counter).await?;

        tx.commit().await?;

        Ok((primary_movement, counter_movement))
    }
}

impl FromRequestParts<AppState> for Repository {
    type Rejection = Infallible;

    async fn from_request_parts(
        _parts: &mut axum::http::request::Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        Ok(Self {
            db: state.db.clone(),
        })
    }
}

#[cfg(test)]
impl From<PgPool> for Repository {
    fn from(db: PgPool) -> Self {
        Self { db }
    }
}
