use axum::{Json, Router, routing::get};
use serde::Deserialize;

use crate::{
    app::AppState, auth::admin::Admin, error::AppError, models::Asset, repository::Repository,
};

pub fn router() -> Router<AppState> {
    Router::new().route(
        "/assets",
        get(list_assets).post(create_asset).patch(update_asset),
    )
}

#[tracing::instrument(skip_all)]
async fn list_assets(repostiory: Repository) -> Result<Json<Vec<Asset>>, AppError> {
    let assets = repostiory.list_assets().await?;
    Ok(Json(assets))
}

#[derive(Deserialize)]
struct CreateAssetRequest {
    name: String,
    unit_value: f64,
    /// "crypto" ou "fiat" — permite cadastrar tanto criptomoedas quanto
    /// moedas fiduciárias (dólar, euro, etc.) no mesmo catálogo.
    asset_type: String,
}

#[tracing::instrument(skip_all)]
async fn create_asset(
    _: Admin,
    repostiory: Repository,
    Json(request): Json<CreateAssetRequest>,
) -> Result<Json<Asset>, AppError> {
    if request.asset_type != "crypto" && request.asset_type != "fiat" {
        return Err(AppError::InvalidAssetType);
    }

    let new_asset = repostiory
        .create_asset(request.name, request.unit_value, request.asset_type)
        .await?;

    Ok(Json(new_asset))
}

#[derive(Deserialize)]
struct UpdateAssetRequest {
    id: i64,
    name: Option<String>,
    unit_value: Option<f64>,
}

#[tracing::instrument(skip_all)]
async fn update_asset(
    _: Admin,
    repostiory: Repository,
    Json(request): Json<UpdateAssetRequest>,
) -> Result<Json<Asset>, AppError> {
    match repostiory
        .update_asset(request.id, request.name, request.unit_value)
        .await?
    {
        Some(updated_asset) => Ok(Json(updated_asset)),
        None => Err(AppError::AssetDoesNotExist),
    }
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::*;

    #[sqlx::test]
    async fn test_create_asset(db: PgPool) {
        let request = CreateAssetRequest {
            name: "Bitcoin".to_string(),
            unit_value: 10.0,
            asset_type: "crypto".to_string(),
        };
        let Json(new_asset) = create_asset(Admin, db.into(), Json(request))
            .await
            .expect("success");

        assert!(new_asset.id > 0);
        assert_eq!(new_asset.name, "Bitcoin");
        assert_eq!(new_asset.unit_value, 10.0);
        assert_eq!(new_asset.asset_type, "crypto");

        // O id não é determinístico (depende das moedas fiduciárias
        // pré-cadastradas pela migration), então não faz parte do snapshot.
        insta::assert_json_snapshot!(serde_json::json!({
            "name": new_asset.name,
            "unit_value": new_asset.unit_value,
            "asset_type": new_asset.asset_type,
        }));
    }

    #[sqlx::test]
    async fn test_create_asset_rejects_invalid_type(db: PgPool) {
        let request = CreateAssetRequest {
            name: "Real".to_string(),
            unit_value: 1.0,
            asset_type: "gold".to_string(),
        };

        let err = create_asset(Admin, db.into(), Json(request))
            .await
            .expect_err("should reject unknown asset_type");

        assert!(matches!(err, AppError::InvalidAssetType));
    }

    #[sqlx::test]
    async fn test_list_assets(db: PgPool) {
        let repository: Repository = db.into();
        repository
            .create_asset("Bitcoin".to_string(), 10.0, "crypto".to_string())
            .await
            .expect("create asset");

        let Json(assets) = list_assets(repository).await.expect("success");

        let bitcoin = assets
            .iter()
            .find(|asset| asset.name == "Bitcoin")
            .expect("bitcoin should be listed");
        assert_eq!(bitcoin.unit_value, 10.0);
        assert_eq!(bitcoin.asset_type, "crypto");

        insta::assert_json_snapshot!(serde_json::json!({
            "name": bitcoin.name,
            "unit_value": bitcoin.unit_value,
            "asset_type": bitcoin.asset_type,
        }));
    }

    #[sqlx::test]
    async fn test_update_asset(db: PgPool) {
        let repository: Repository = db.into();
        let bitcoin = repository
            .create_asset("Bitcoin".to_string(), 10.0, "crypto".to_string())
            .await
            .expect("create asset");

        let request = UpdateAssetRequest {
            id: bitcoin.id,
            name: Some("Ethereum".to_string()),
            unit_value: Some(20.0),
        };

        let Json(updated_asset) = update_asset(Admin, repository, Json(request))
            .await
            .expect("success");

        assert_eq!(updated_asset.id, bitcoin.id);
        assert_eq!(updated_asset.name, "Ethereum");
        assert_eq!(updated_asset.unit_value, 20.0);

        insta::assert_json_snapshot!(serde_json::json!({
            "name": updated_asset.name,
            "unit_value": updated_asset.unit_value,
            "asset_type": updated_asset.asset_type,
        }));
    }
}
