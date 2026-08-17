use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Serialize, Clone)]
pub struct Asset {
    pub id: i64,
    pub name: String,
    pub unit_value: f64,
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
}

#[derive(Serialize, Clone)]
pub struct Movement {
    pub id: i64,
    pub kind: String,
    pub quantity: f64,
    pub unit_price: f64,
    pub created_at: DateTime<Utc>,
}
