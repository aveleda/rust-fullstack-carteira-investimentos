# Carteira de Investimentos (wallet-live)

Aplicação web em Rust (Axum + Askama + SQLx/Postgres) para gerenciar uma carteira de investimentos: login com sessão via JWT, catálogo de criptomoedas e moedas fiduciárias, compra/venda entre elas e depósito em Real, com resumo e histórico de movimentações por moeda.

Para o diagnóstico e o histórico detalhado de cada melhoria implementada, veja [docs/analise-melhorias.md](docs/analise-melhorias.md) e [docs/resumo-implementacao.md](docs/resumo-implementacao.md).

## Pré-requisitos

- [Rust](https://www.rust-lang.org/tools/install) estável (edição 2024 — testado com `rustc 1.97`). Instale via `rustup`.
- PostgreSQL 14+ acessível (local ou via Docker).
- [sqlx-cli](https://github.com/launchbadge/sqlx/tree/main/sqlx-cli), para rodar as migrations:
  ```bash
  cargo install sqlx-cli --no-default-features --features postgres
  ```
- `openssl` (opcional, só para gerar valores aleatórios dos secrets abaixo).

## 1. Banco de dados

### Subir o Postgres

O `compose.yml` na raiz sobe um Postgres vazio (role `postgres`/`postgres`, porta `5432`):

```bash
docker compose up -d
```

Se preferir, use uma instância Postgres já existente — só ajuste os comandos abaixo para o usuário/host correto.

### Criar a role e o banco da aplicação

O `compose.yml` não cria a role/banco que a aplicação usa (`invest`/`invest`) — crie manualmente uma vez, conectando como o superusuário (`postgres`/`postgres` no compose acima):

```bash
psql "postgres://postgres:postgres@localhost:5432/postgres" <<'SQL'
CREATE ROLE invest WITH LOGIN PASSWORD 'invest';
CREATE DATABASE invest OWNER invest;
SQL
```

### Aplicar as migrations

```bash
DATABASE_URL=postgres://invest:invest@localhost:5432/invest sqlx migrate run
```

Isso cria as tabelas (`users`, `assets`, `movements`) e já semeia três moedas fiduciárias (`Real`, `Dolar Americano`, `Euro`) — o `Real` é a moeda-âncora (vale sempre R$ 1,00) e a única que aceita depósito direto.

## 2. Configuração (`.env`)

Copie o exemplo e ajuste os valores:

```bash
cp .env.example .env
```

```
DATABASE_URL=postgres://invest:invest@localhost:5432/invest
JWT_SECRET=<valor aleatório longo>
ADMIN_SECRET_KEY=<valor aleatório longo>
```

- `JWT_SECRET` assina os tokens de sessão; `ADMIN_SECRET_KEY` protege as rotas administrativas de catálogo (ver seção 5). Gere valores com:
  ```bash
  openssl rand -hex 32   # JWT_SECRET
  openssl rand -hex 24   # ADMIN_SECRET_KEY
  ```
- O `.env` não é versionado (está no `.gitignore`) — cada ambiente deve ter o seu.

## 3. Compilar e executar

```bash
cargo build          # compilar
cargo run            # compilar (se necessário) e executar
```

O servidor sobe em `http://localhost:3000`. Abra essa URL no navegador — a tela de login permite criar um usuário novo (login com um usuário inexistente cadastra automaticamente).

Para build otimizado de produção: `cargo build --release` (binário em `target/release/wallet-live`).

## 4. Rodar os testes

Os testes (`#[sqlx::test]`) criam um banco efêmero por teste, o que exige uma role com privilégio `CREATEDB` — privilégio que a role `invest` (produção) **não** tem, de propósito. Configure uma role dedicada só para testes:

```bash
psql "postgres://postgres:postgres@localhost:5432/postgres" -f scripts/setup_test_role.sql
DATABASE_URL=postgres://invest_test:invest_test@localhost:5432/invest_test sqlx migrate run
```

E rode os testes apontando para ela:

```bash
DATABASE_URL=postgres://invest_test:invest_test@localhost:5432/invest_test cargo test
```

(Veja [.env.test.example](.env.test.example) e [docs/resumo-implementacao.md](docs/resumo-implementacao.md) — seção 2 — para o porquê dessa separação.)

Outras verificações úteis durante o desenvolvimento:

```bash
cargo clippy --all-targets   # lints
cargo fmt                    # formatação
```

## 5. Administrar o catálogo de moedas

Não há tela de administração — o catálogo (`assets`) é gerenciado via API, autenticada com o header `Authorization: <ADMIN_SECRET_KEY>`:

```bash
curl -X POST http://localhost:3000/api/assets \
  -H "Authorization: $ADMIN_SECRET_KEY" \
  -H "Content-Type: application/json" \
  -d '{"name":"Solana","unit_value":900,"asset_type":"crypto"}'
```

- `asset_type` deve ser `"crypto"` ou `"fiat"`.
- `GET /api/assets` lista o catálogo (rota pública); `PATCH /api/assets` atualiza nome/valor de um ativo existente (também exige o header de admin).

## 6. Estrutura do projeto

```
src/
  app.rs           - estado da aplicação (pool do banco, chave JWT) e montagem do router
  auth/             - autenticação de usuário (JWT) e do admin (shared secret)
  error.rs          - erros da aplicação e mapeamento para status HTTP
  models.rs         - structs de domínio (Asset, Movement, Holding, ...)
  repository.rs     - acesso ao banco (SQLx)
  routes/
    api.rs          - API JSON de administração do catálogo (/api/assets)
    frontend.rs      - páginas HTML (login, dashboard, extrato, compra/venda/depósito)
migrations/         - migrations SQL (sqlx-cli)
templates/          - templates Askama (Tailwind via CDN, sem build step de CSS/JS)
scripts/            - scripts auxiliares (setup da role de testes)
docs/               - documentação de análise e das melhorias implementadas
```
