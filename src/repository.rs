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

impl Repository {
    pub async fn list_assets(&self) -> sqlx::Result<Vec<Asset>> {
        sqlx::query_as!(
            Asset,
            "SELECT id, name, unit_value
             FROM assets;"
        )
        .fetch_all(&self.db)
        .await
    }

    pub async fn create_asset(&self, name: String, unit_value: f64) -> sqlx::Result<Asset> {
        sqlx::query_as!(
            Asset,
            "INSERT INTO assets (name, unit_value)
             VALUES ($1, $2)
             RETURNING id, name, unit_value;",
            name,
            unit_value
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
             RETURNING id, name, unit_value;",
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
            "SELECT id, name, unit_value
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
                      SUM(CASE WHEN m.kind = 'buy' THEN m.quantity ELSE -m.quantity END) AS "quantity!: f64"
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

    pub async fn list_movements(&self, user_id: i64, asset_id: i64) -> sqlx::Result<Vec<Movement>> {
        sqlx::query_as!(
            Movement,
            "SELECT id, kind, quantity, unit_price, created_at
             FROM movements
             WHERE user_id = $1 AND asset_id = $2
             ORDER BY created_at DESC;",
            user_id,
            asset_id
        )
        .fetch_all(&self.db)
        .await
    }

    pub async fn create_movement(
        &self,
        user_id: i64,
        asset_id: i64,
        kind: &str,
        quantity: f64,
        unit_price: f64,
    ) -> sqlx::Result<Movement> {
        sqlx::query_as!(
            Movement,
            "INSERT INTO movements (user_id, asset_id, kind, quantity, unit_price)
             VALUES ($1, $2, $3, $4, $5)
             RETURNING id, kind, quantity, unit_price, created_at;",
            user_id,
            asset_id,
            kind,
            quantity,
            unit_price
        )
        .fetch_one(&self.db)
        .await
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
