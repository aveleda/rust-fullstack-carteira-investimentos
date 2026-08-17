# Resumo da implementação — melhorias na Carteira de Investimentos

Data: 2026-08-17
Documento complementar a [analise-melhorias.md](analise-melhorias.md) (que traz o diagnóstico e a justificativa de cada decisão). Este documento resume **o que foi efetivamente implementado** e **o que precisa ser executado no PostgreSQL** para o projeto funcionar corretamente em qualquer ambiente.

## 1. Resumo do que foi implementado

### 1.1 Sessão via JWT
- `JWT_SECRET` e `ADMIN_SECRET_KEY` deixaram de ser constantes no código-fonte e passaram a ser lidos de variáveis de ambiente, carregados uma única vez em `AppState` ([src/app.rs](../src/app.rs)).
- `.env` removido do controle de versão (continha segredos) e adicionado ao `.gitignore`; [.env.example](../.env.example) criado com placeholders para novos ambientes.
- Duração da sessão: de 10 minutos para 2 horas (`session_duration()` em [src/auth/user.rs](../src/auth/user.rs)).
- Cookie de sessão agora define `SameSite=Lax`, `Path=/` e `max_age` alinhado à expiração do JWT (antes só tinha `HttpOnly`).
- Nova rota `GET /logout`, que remove o cookie e redireciona para `/login`.
- Toda rota que exibe dados da carteira exige o extractor `User` (sessão obrigatória, não opcional).

### 1.2 Usuário com várias moedas + histórico de movimentação
- Nova tabela `movements` (ledger de compra/venda por usuário e moeda) — ver comandos SQL na seção 2.
- `Repository::list_user_holdings` agrega as movimentações do usuário para calcular a posse atual de cada moeda (compras − vendas), sem manter um saldo duplicado.
- `Repository::list_movements(user_id, asset_id)` retorna o histórico de uma moeda, **sempre filtrado pelo usuário autenticado** — testado manualmente que um usuário não consegue ver o histórico de outro (proteção contra IDOR).
- `Repository::create_movement` registra uma nova movimentação (usada hoje pela rota de compra).
- Novas rotas em [src/routes/frontend.rs](../src/routes/frontend.rs):
  - `GET /` — dashboard com as moedas do usuário autenticado.
  - `GET /assets/{id}` — histórico de movimentações daquela moeda para o usuário autenticado.
  - `POST /assets/{id}/buy` — registra uma compra (quantidade via formulário, preço unitário fixado a partir do valor atual do asset).

### 1.3 Frontend pós-login com Tailwind CDN
- [templates/base.html](../templates/base.html) — layout compartilhado que carrega `https://cdn.tailwindcss.com` e as fontes uma única vez.
- [templates/login.html](../templates/login.html) — passou a estender `base.html` (eliminou a duplicação do `<head>`).
- [templates/dashboard.html](../templates/dashboard.html) — lista as moedas do usuário e um formulário de compra por moeda do catálogo.
- [templates/asset_history.html](../templates/asset_history.html) — histórico de movimentações da moeda selecionada.
- Nenhuma página tem mais de um `<script>` (o próprio CDN do Tailwind); navegação entre dashboard e histórico é feita por link normal (`<a href>`), sem JavaScript customizado.

### 1.4 Verificações realizadas
- `cargo build` e `cargo clippy --all-targets` sem erros/avisos.
- Fluxo validado manualmente via `curl`: login/registro automático, listagem de moedas, compra, histórico, isolamento entre usuários e logout.
- Os 3 testes `sqlx::test` pré-existentes (`test_create_asset`, `test_list_assets`, `test_update_asset`) continuam falhando no ambiente local — não por causa das mudanças desta rodada, mas por uma permissão faltante no PostgreSQL (ver seção 2.2).

## 2. Alterações necessárias no PostgreSQL

### 2.1 Nova migration: tabela `movements`

Já aplicada neste ambiente via `sqlx migrate run`. Os arquivos ficam em `migrations/` e podem ser reaplicados em qualquer ambiente com o comando:

```bash
sqlx migrate run
```

O SQL executado foi:

```sql
-- migrations/20260817120000_create_movements.up.sql
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
```

Para desfazer (rollback), caso necessário:

```bash
sqlx migrate revert
```

que executa:

```sql
-- migrations/20260817120000_create_movements.down.sql
DROP TABLE IF EXISTS movements;
```

Estado confirmado no banco local após a migration:

```
 Schema |       Name       | Type  | Owner
--------+------------------+-------+--------
 public | _sqlx_migrations | table | invest
 public | assets           | table | invest
 public | movements        | table | invest
 public | users            | table | invest
```

### 2.2 Role dedicada para os testes (`invest_test`) — implementado e validado

A role de aplicação `invest` **não tem** a permissão `CREATEDB`:

```sql
SELECT rolname, rolcreatedb, rolcreaterole, rolsuper FROM pg_roles WHERE rolname = 'invest';
--  rolname | rolcreatedb | rolcreaterole | rolsuper
-- ---------+-------------+---------------+----------
--  invest  | f           | f             | f
```

Isso é o motivo dos testes que usam `#[sqlx::test]` falharem com `permission denied to create database`: esse macro do sqlx **cria um banco de dados novo a cada teste** (via `CREATE DATABASE`), aplica as migrations nele, roda o teste isolado ali, e depois descarta o banco — é assim que ele garante isolamento entre testes que rodam em paralelo, sem sujar o banco real. Essa limitação já existia antes desta rodada de melhorias (os testes de `assets` já usavam o mesmo mecanismo).

`CREATE DATABASE`/`DROP DATABASE` são operações de nível de **cluster** no PostgreSQL, controladas pelo atributo de role `CREATEDB` — não por um `GRANT` dentro de um banco específico. Ou seja, não basta a role `invest` ter acesso total ao seu próprio banco `invest`; a permissão para criar *outros* bancos é um nível de privilégio diferente.

**Decisão adotada:** em vez de dar `CREATEDB` à role de produção `invest` (o que ampliaria seu raio de ação além do necessário), foi criada uma role **separada**, `invest_test`, usada apenas para rodar `cargo test`. A role `invest` usada pela aplicação em runtime continua sem `CREATEDB`.

Comandos para configurar o ambiente de testes — todos em [scripts/setup_test_role.sql](../scripts/setup_test_role.sql), executados **por um superusuário** do PostgreSQL (ex.: role `postgres`), já que criar roles/bancos exige esse nível de privilégio:

```sql
-- 1. Role dedicada aos testes, com permissão de criar bancos efêmeros
CREATE ROLE invest_test WITH LOGIN PASSWORD 'invest_test' CREATEDB;

-- 2. Banco inicial de conexão para essa role
--    (o #[sqlx::test] precisa de um banco existente para abrir a conexão
--    inicial, antes de criar os bancos efêmeros por teste)
CREATE DATABASE invest_test OWNER invest_test;
```

Via linha de comando (sem abrir o `psql` interativamente):

```bash
psql "postgres://postgres:<senha_do_superuser>@localhost:5432/postgres" \
  -f scripts/setup_test_role.sql
```

Depois de criado o banco `invest_test`, aplique as migrations nele (necessário porque as macros `query_as!` são checadas em tempo de compilação contra o schema do banco apontado por `DATABASE_URL`):

```bash
DATABASE_URL=postgres://invest_test:invest_test@localhost:5432/invest_test sqlx migrate run
```

Para rodar os testes, use essa `DATABASE_URL` (não a de produção) — veja [.env.test.example](../.env.test.example):

```bash
DATABASE_URL=postgres://invest_test:invest_test@localhost:5432/invest_test cargo test
```

Para confirmar que a separação de privilégios ficou correta:

```sql
SELECT rolname, rolcreatedb FROM pg_roles WHERE rolname IN ('invest', 'invest_test');
--   rolname   | rolcreatedb
-- ------------+-------------
--  invest     | f      -- role de produção: sem CREATEDB
--  invest_test| t      -- role de testes: com CREATEDB
```

Manutenção: se um teste for interrompido (ex.: `Ctrl+C` no meio da execução), o banco efêmero criado por ele pode não ser removido. Para localizar e limpar bancos de teste órfãos:

```sql
-- Como invest_test ou superuser
SELECT datname FROM pg_database WHERE datname LIKE '_sqlx_test%';

-- Remover manualmente cada um encontrado
DROP DATABASE "_sqlx_test_<sufixo>";
```

### 2.3 Variáveis de ambiente relacionadas ao banco (para referência)

Nenhuma mudança de schema além da migration acima foi necessária. As únicas variáveis novas adicionadas ao `.env` (fora do escopo estritamente "PostgreSQL", mas necessárias para a aplicação subir) foram:

```
JWT_SECRET=<valor aleatório longo, ex.: saída de `openssl rand -hex 32`>
ADMIN_SECRET_KEY=<valor aleatório longo, ex.: saída de `openssl rand -hex 24`>
```

`DATABASE_URL` continua apontando para o mesmo banco/role já existentes (`postgres://invest:invest@localhost:5432/invest`), sem necessidade de criar um novo usuário ou banco.

## 3. Validação final

Após a criação da role `invest_test` (seção 2.2), foi validado neste ambiente:

- `SELECT rolname, rolcreatedb FROM pg_roles WHERE rolname IN ('invest', 'invest_test');` confirma `invest` sem `CREATEDB` e `invest_test` com `CREATEDB`.
- O banco `invest_test` existe, tem as migrations aplicadas (`assets`, `users`, `movements`) e nenhum banco de teste órfão ficou para trás (`SELECT datname FROM pg_database WHERE datname LIKE '_sqlx_test%';` retorna vazio).
- `DATABASE_URL=postgres://invest_test:invest_test@localhost:5432/invest_test cargo test` — **os 3 testes passam** (`test_create_asset`, `test_list_assets`, `test_update_asset`).
- Com a `DATABASE_URL` de produção, `cargo build` e `cargo clippy --all-targets` continuam sem erros/avisos.
- Fluxo funcional validado via `curl` novamente após as mudanças: login/registro automático → cookie de sessão (`HttpOnly`, `SameSite=Lax`, `Max-Age=7200`) → dashboard (`200`) → `/logout` (`303` com cookie expirado) → dashboard sem sessão redireciona para `/login` (`303`).

Estado atual: todas as melhorias propostas estão implementadas e verificadas — sessão JWT segura e com logout, modelo de dados usuário↔moeda com histórico e isolamento entre usuários, frontend pós-login com Tailwind CDN único, e a separação de privilégios de banco entre a role de produção e a role de testes.
