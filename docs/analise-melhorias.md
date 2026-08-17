# Análise e plano de melhorias — Carteira de Investimentos (wallet-live)

Data da análise: 2026-08-17

## 1. Estado do projeto antes das melhorias

Stack: Axum 0.8 + Askama (templates) + SQLx/Postgres + `jwt-simple` + `password-auth`. Projeto didático (histórico de commits em "aulas"), arquitetura mínima.

### 1.1 Autenticação / sessão

- Login (`POST /login`) autentica ou **registra automaticamente** um usuário novo (sem tela de cadastro separada).
- JWT já existia (`src/auth/user.rs`), mas com problemas:
  - Chave secreta `SECRET_KEY` **hardcoded no binário** (`b"im-so-secret"`).
  - Senha de admin (`ADMIN_SECRET_KEY`, `src/auth/admin.rs`) também hardcoded, comparada direto contra o header `Authorization`.
  - Expiração fixa de 10 minutos, sem refresh e sem `/logout`.
  - Cookie sem `Secure`/`SameSite`/`max_age` explícitos.

### 1.2 Modelo de dados

- Tabela `assets` era um catálogo **global** (sem relação com usuário).
- Não existia nenhuma relação usuário↔moeda (nem N:N, nem holdings) e **não existia** tabela de movimentação/histórico.
- A página `/` pós-login era um placeholder de texto puro (`Hello, {username}`), sem listar nada da carteira do usuário.

### 1.3 Frontend

- Único template: `login.html`, já usando `https://cdn.tailwindcss.com` via `<script>` e Google Fonts, sem JS customizado.
- Não havia layout compartilhado nem página de dashboard/histórico.

## 2. Melhorias implementadas

### 2.1 Sessão JWT

- `JWT_SECRET` e `ADMIN_SECRET_KEY` passaram a ser lidos de variáveis de ambiente (`.env`) e carregados uma única vez em `AppState`, em vez de constantes no código-fonte.
- Adicionado `GET /logout`, que remove o cookie de sessão.
- Cookie de sessão agora define `SameSite=Lax`, `Path=/` e `max_age` alinhado ao tempo de expiração do JWT.
- Extensão do tempo de sessão de 10 minutos para 2 horas (compromisso razoável para uma aplicação sem fluxo de refresh token; documentado aqui como decisão consciente — um refresh token dedicado é uma melhoria futura).
- Toda rota que exibe dados da carteira do usuário exige o extractor `User` (obrigatório, não `Option<User>`).

### 2.2 Modelo de dados: usuário com várias moedas + histórico de movimentação

Modelagem adotada: **ledger de movimentações** (`movements`), em vez de um saldo mutável separado — evita divergência entre saldo e histórico.

- Nova tabela `movements`: `id, user_id (FK users), asset_id (FK assets), kind ('buy'|'sell'), quantity, unit_price, created_at`.
- Posse de cada usuário (quais moedas ele tem e em que quantidade) é **derivada** por agregação: soma de compras menos vendas, agrupada por `asset_id`.
- Todas as consultas de histórico filtram obrigatoriamente por `user_id = usuário autenticado` além do `asset_id`, evitando que um usuário veja o histórico de outro (proteção contra IDOR).

### 2.3 Frontend pós-login

- Novo template base (`base.html`) com o Tailwind CDN e as fontes carregados **uma única vez**, herdado pelas demais páginas — nenhum script adicional além do CDN do Tailwind em nenhuma página.
- `GET /` (dashboard): lista as moedas que o usuário possui (com quantidade) e um formulário simples para comprar moedas do catálogo — tudo via submissão de formulário HTML padrão (POST), sem JavaScript customizado.
- `GET /assets/{id}`: histórico de movimentações daquela moeda para o usuário autenticado, com verificação de propriedade.
- Navegação entre dashboard e histórico é feita por link normal (`<a href>`), ou seja, "selecionar uma moeda" é uma navegação de página comum renderizada no servidor — mantém a contagem de scripts na página igual à da tela de login (apenas o CDN do Tailwind).

## 3. Pontos deixados como melhoria futura (fora do escopo desta rodada)

- Refresh token / renovação silenciosa de sessão.
- Autorização de admin baseada em papéis (hoje ainda é um shared-secret comparado a um header).
- Uso de `NUMERIC`/`DECIMAL` em vez de `DOUBLE PRECISION` para valores monetários.
- Tela de cadastro dedicada (hoje login com usuário inexistente ainda registra automaticamente).
- CSRF protection nos formulários HTML.
