use askama::Template;
use axum::{
    Form, Router,
    extract::{Path, State},
    response::{Html, IntoResponse, Redirect, Response},
    routing::get,
};
use axum_extra::extract::{
    CookieJar,
    cookie::{Cookie, SameSite},
};
use serde::Deserialize;

use crate::{
    app::AppState,
    auth::user::{UnauthenticatedUser, User, session_duration},
    error::AppError,
    models::{Asset, Holding, Movement},
    repository::{NewMovement, Repository},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(index))
        .route("/login", get(login_page).post(login))
        .route("/logout", get(logout))
        .route("/assets/{id}", get(asset_history))
        .route("/assets/{id}/buy", axum::routing::post(buy_asset))
}

#[derive(Template)]
#[template(path = "login.html")]
struct LoginPage;

async fn login_page() -> Result<Html<String>, AppError> {
    let html = LoginPage.render()?;
    Ok(Html(html))
}

#[derive(Deserialize)]
struct LoginForm {
    username: String,
    password: String,
}

async fn login(
    State(state): State<AppState>,
    repository: Repository,
    jar: CookieJar,
    Form(request): Form<LoginForm>,
) -> Result<impl IntoResponse, AppError> {
    let unauth_user = UnauthenticatedUser::new(request.username, request.password);
    let user = match unauth_user.authenticate(&repository).await {
        Ok(user) => user,
        Err(AppError::UserDoesNotExist) => unauth_user.register(&repository).await?,
        Err(other_err) => return Err(other_err),
    };

    let token = user.auth_token(&state.jwt_key)?;
    let cookie = Cookie::build(("token", token))
        .http_only(true)
        .same_site(SameSite::Lax)
        .path("/")
        .max_age(time::Duration::seconds(session_duration().as_secs() as i64));

    Ok((jar.add(cookie), Redirect::to("/")))
}

async fn logout(jar: CookieJar) -> impl IntoResponse {
    (jar.remove(Cookie::from("token")), Redirect::to("/login"))
}

#[derive(Template)]
#[template(path = "dashboard.html")]
struct DashboardPage {
    username: String,
    holdings: Vec<Holding>,
    /// Soma, em reais, do preço médio pago por cada moeda em carteira.
    total_invested: f64,
    /// Soma, em reais, do valor atual de cada moeda em carteira.
    total_current: f64,
    crypto_assets: Vec<Asset>,
    fiat_assets: Vec<Asset>,
    /// Catálogo completo, usado para preencher o seletor de moeda de pagamento.
    assets: Vec<Asset>,
}

async fn index(maybe_user: Option<User>, repository: Repository) -> Result<Response, AppError> {
    let Some(user) = maybe_user else {
        return Ok(Redirect::to("/login").into_response());
    };

    let holdings = repository.list_user_holdings(user.id()).await?;
    let assets = repository.list_assets().await?;

    let total_invested = holdings.iter().map(|h| h.quantity * h.avg_unit_price).sum();
    let total_current = holdings.iter().map(|h| h.quantity * h.unit_value).sum();

    let crypto_assets = assets
        .iter()
        .filter(|asset| asset.asset_type == "crypto")
        .cloned()
        .collect();
    let fiat_assets = assets
        .iter()
        .filter(|asset| asset.asset_type == "fiat")
        .cloned()
        .collect();

    let page = DashboardPage {
        username: user.username().clone(),
        holdings,
        total_invested,
        total_current,
        crypto_assets,
        fiat_assets,
        assets,
    };

    Ok(Html(page.render()?).into_response())
}

#[derive(Template)]
#[template(path = "asset_history.html")]
struct AssetHistoryPage {
    asset_name: String,
    movements: Vec<Movement>,
}

async fn asset_history(
    user: User,
    repository: Repository,
    Path(asset_id): Path<i64>,
) -> Result<Html<String>, AppError> {
    let asset = repository
        .get_asset(asset_id)
        .await?
        .ok_or(AppError::AssetDoesNotExist)?;

    let movements = repository.list_movements(user.id(), asset_id).await?;

    let page = AssetHistoryPage {
        asset_name: asset.name,
        movements,
    };

    Ok(Html(page.render()?))
}

#[derive(Deserialize)]
struct BuyForm {
    quantity: f64,
    paid_amount: f64,
    paid_currency_id: i64,
}

async fn buy_asset(
    user: User,
    repository: Repository,
    Path(asset_id): Path<i64>,
    Form(request): Form<BuyForm>,
) -> Result<impl IntoResponse, AppError> {
    if request.quantity <= 0.0 || request.paid_amount <= 0.0 {
        return Err(AppError::InvalidQuantity);
    }

    if request.paid_currency_id == asset_id {
        return Err(AppError::InvalidCurrency);
    }

    repository
        .get_asset(asset_id)
        .await?
        .ok_or(AppError::AssetDoesNotExist)?;

    let currency = repository
        .get_asset(request.paid_currency_id)
        .await?
        .ok_or(AppError::InvalidCurrency)?;

    let unit_price_brl = request.paid_amount * currency.unit_value / request.quantity;

    repository
        .create_movement(
            user.id(),
            NewMovement {
                asset_id,
                kind: "buy",
                quantity: request.quantity,
                unit_price: unit_price_brl,
                paid_amount: request.paid_amount,
                paid_currency_id: currency.id,
            },
        )
        .await?;

    Ok(Redirect::to("/"))
}
