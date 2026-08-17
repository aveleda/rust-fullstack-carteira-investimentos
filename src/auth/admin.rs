use axum::{extract::FromRequestParts, http::header::AUTHORIZATION};

use crate::{app::AppState, error::AppError};

pub struct Admin;

impl FromRequestParts<AppState> for Admin {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let Some(auth) = parts.headers.get(AUTHORIZATION) else {
            return Err(AppError::MissingAuthorization);
        };

        if auth == state.admin_secret.as_str() {
            Ok(Admin)
        } else {
            Err(AppError::InvalidCredentials)
        }
    }
}
