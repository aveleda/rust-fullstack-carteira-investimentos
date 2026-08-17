use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Debug, Serialize, Clone)]
pub struct Asset {
    pub id: i64,
    pub name: String,
    pub unit_value: f64,
    pub asset_type: String,
}

pub struct UserRecord {
    pub id: i64,
    pub username: String,
    pub password_hash: String,
}

#[derive(Serialize, Clone)]
pub struct Holding {
    pub asset_id: i64,
    pub name: String,
    pub unit_value: f64,
    pub quantity: f64,
    /// Preço médio de compra, em reais, ponderado pela quantidade comprada.
    pub avg_unit_price: f64,
}

#[derive(Serialize, Clone)]
pub struct Movement {
    pub id: i64,
    pub kind: String,
    pub quantity: f64,
    /// Preço unitário no momento da movimentação, em reais.
    pub unit_price: f64,
    /// Valor efetivamente pago, na moeda de pagamento (`paid_currency_name`).
    pub paid_amount: f64,
    pub paid_currency_name: String,
    pub created_at: DateTime<Utc>,
}
