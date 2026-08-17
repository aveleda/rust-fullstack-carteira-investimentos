use axum::Router;
use jwt_simple::prelude::HS256Key;
use sqlx::PgPool;
use tokio::net::TcpListener;
use tracing::info;
use tracing_subscriber::{
    Layer, fmt::format::FmtSpan, layer::SubscriberExt, util::SubscriberInitExt,
};

use crate::routes;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub jwt_key: HS256Key,
    pub admin_secret: String,
}

impl AppState {
    async fn new() -> color_eyre::Result<Self> {
        let database_url = std::env::var("DATABASE_URL")?;
        let db = PgPool::connect(&database_url).await?;

        let jwt_secret = std::env::var("JWT_SECRET")?;
        let jwt_key = HS256Key::from_bytes(jwt_secret.as_bytes());

        let admin_secret = std::env::var("ADMIN_SECRET_KEY")?;

        Ok(Self {
            db,
            jwt_key,
            admin_secret,
        })
    }
}

pub struct App;

impl App {
    pub async fn start() -> color_eyre::Result<()> {
        let layer = tracing_subscriber::fmt::layer()
            .with_span_events(FmtSpan::NEW)
            .boxed();

        tracing_subscriber::registry().with(layer).init();

        dotenvy::dotenv()?;
        let state = AppState::new().await?;

        let listener = TcpListener::bind("0.0.0.0:3000").await?;
        let router = Router::new()
            .nest("/api", routes::api::router())
            .merge(routes::frontend::router())
            .with_state(state);

        info!("Starting service");

        axum::serve(listener, router).await?;

        Ok(())
    }
}
