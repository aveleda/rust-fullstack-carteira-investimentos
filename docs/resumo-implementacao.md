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

## 4. Rodada 2 — múltiplas moedas, valor pago e resumo condensado

Data: 2026-08-17

### 4.1 Catálogo com criptomoedas e moedas fiduciárias
- Nova coluna `assets.asset_type` (`crypto` ou `fiat`), validada em `POST /api/assets` — um tipo fora desse conjunto retorna `400 Bad Request` (`AppError::InvalidAssetType`).
- A migration da seção 4.4 semeia três moedas fiduciárias padrão: `Real` (âncora, R$ 1,00), `Dolar Americano` (R$ 5,20) e `Euro` (R$ 5,60). Novas criptomoedas ou moedas fiduciárias são cadastradas pelo mesmo endpoint admin já existente, agora informando `asset_type`.
- O dashboard passou a exibir dois catálogos de compra separados — "Comprar criptomoedas" e "Comprar moedas fiduciárias" — filtrando `assets` por `asset_type` em [src/routes/frontend.rs](../src/routes/frontend.rs).

### 4.2 Valor e moeda usados na compra
- `movements` ganhou `paid_amount` (quanto foi efetivamente pago) e `paid_currency_id` (em qual moeda do catálogo) — ver migration na seção 4.4.
- O formulário de compra ([templates/dashboard.html](../templates/dashboard.html)) pede quantidade, valor pago e a moeda de pagamento (qualquer outra moeda do catálogo, cripto ou fiduciária).
- `unit_price` (sempre em reais, para manter a base de cálculo do preço médio) é derivado no momento da compra: `paid_amount × unit_value_da_moeda_escolhida / quantidade`. Isso permite pagar em qualquer moeda (ex.: comprar Ethereum pagando em dólares) sem perder a base em reais.
- Validações: `quantity`/`paid_amount` devem ser positivos, e a moeda de pagamento não pode ser o próprio ativo comprado (`AppError::InvalidCurrency`, `400`).
- O histórico ([templates/asset_history.html](../templates/asset_history.html)) mostra, por movimentação, o preço unitário em reais **e** "pago: `<valor>` `<moeda>`".

### 4.3 Resumo condensado com valor médio em reais
- `Holding` (agregação por moeda) ganhou `avg_unit_price`: preço médio de compra em reais, ponderado pela quantidade comprada (`SUM(quantity*unit_price) / SUM(quantity)` sobre as compras).
- O dashboard exibe, por moeda em carteira: quantidade, preço médio (R$) e preço atual (R$).
- Um card de resumo no topo do dashboard soma, em reais, o valor investido (`Σ quantidade × preço médio`) e o valor atual (`Σ quantidade × preço atual`), e mostra o resultado (lucro/perda) em verde ou vermelho.
- Simplificação assumida: o preço médio considera apenas movimentações de compra (a UI ainda não expõe venda); documentado aqui para não ser esquecido caso a venda seja implementada depois.

### 4.4 Nova migration

```sql
-- migrations/20260817130000_add_asset_type_and_payment_currency.up.sql
ALTER TABLE assets
 ADD COLUMN asset_type TEXT NOT NULL DEFAULT 'crypto' CHECK (asset_type IN ('crypto', 'fiat'));

ALTER TABLE assets ALTER COLUMN asset_type DROP DEFAULT;

INSERT INTO assets (name, unit_value, asset_type) VALUES
 ('Real', 1, 'fiat'),
 ('Dolar Americano', 5.2, 'fiat'),
 ('Euro', 5.6, 'fiat')
ON CONFLICT (name) DO NOTHING;

ALTER TABLE movements
 ADD COLUMN paid_amount DOUBLE PRECISION,
 ADD COLUMN paid_currency_id BIGINT REFERENCES assets (id);

-- Movimentações registradas antes desta migration não têm moeda de
-- pagamento explícita; assume-se que o preço já estava em reais.
UPDATE movements
SET paid_amount = quantity * unit_price,
    paid_currency_id = (SELECT id FROM assets WHERE name = 'Real')
WHERE paid_amount IS NULL;

ALTER TABLE movements
 ALTER COLUMN paid_amount SET NOT NULL,
 ALTER COLUMN paid_currency_id SET NOT NULL;

ALTER TABLE movements ADD CONSTRAINT movements_paid_amount_positive CHECK (paid_amount > 0);
```

Aplicada nos dois bancos deste ambiente:

```bash
DATABASE_URL=postgres://invest:invest@localhost:5432/invest sqlx migrate run
DATABASE_URL=postgres://invest_test:invest_test@localhost:5432/invest_test sqlx migrate run
```

Rollback, se necessário (`sqlx migrate revert`): remove as colunas novas de `movements` e `asset_type` de `assets`, mas **não** remove as moedas fiduciárias semeadas (linhas de catálogo inofensivas de se manter).

### 4.5 Testes

Como a migration passou a semear moedas fiduciárias em todo banco recém-migrado (inclusive nos bancos efêmeros que o `#[sqlx::test]` cria), os testes de asset não podem mais assumir `id`s fixos (ex.: "o primeiro asset criado tem id 1"). Ajustes em [src/routes/api.rs](../src/routes/api.rs):

- `test_create_asset`, `test_list_assets` e `test_update_asset` passaram a criar o asset dinamicamente dentro do próprio teste (em vez do fixture `bitcoin_asset.sql`, removido) e a excluir o `id` do snapshot do `insta` (comparando só `name`/`unit_value`/`asset_type`).
- Novo teste `test_create_asset_rejects_invalid_type`, cobrindo a validação da seção 4.1.
- `cargo test` com a role dedicada `invest_test` (seção 2.2): **4/4 testes passam**.

### 4.6 Validação manual

Fluxo testado via `curl` nesta rodada:
- Compra de Ethereum pagando com Dólar Americano e, em seguida, com Real — preço unitário em reais calculado corretamente em cada caso a partir da cotação da moeda de pagamento.
- Preço médio ponderado e resumo condensado (investido/atual/resultado) conferem com o cálculo manual esperado.
- Tentativa de pagar um ativo com ele mesmo → `400`. Cadastro de tipo de asset inválido via API admin → `400`. Cadastro de nova moeda fiduciária (Libra Esterlina) e nova criptomoeda (Solana) via API admin → `200`.
- `cargo build`, `cargo clippy --all-targets` e `cargo fmt` (nos arquivos alterados) sem erros/avisos.

## 5. Rodada 3 — venda de moedas e frações menores que 0,01

Data: 2026-08-17

### 5.1 Venda de moedas
- Nova rota `POST /assets/{id}/sell`, simétrica à de compra: recebe quantidade, valor recebido e em qual moeda o vendedor foi pago. O preço unitário em reais é calculado do mesmo jeito que na compra (`valor_recebido × unit_value_da_moeda / quantidade`).
- A validação comum a compra e venda (quantidade/valor positivos, moeda de troca diferente do próprio ativo, moeda existente) foi extraída para `validate_and_price_trade` em [src/routes/frontend.rs](../src/routes/frontend.rs), reaproveitada por `buy_asset` e `sell_asset`.
- Antes de gravar uma venda, `Repository::get_holding_quantity` (nova, em [src/repository.rs](../src/repository.rs)) confere quanto o usuário possui daquele ativo; vender mais do que se tem retorna `400` (`AppError::InsufficientHoldings`).
- O preço médio de compra (`avg_unit_price`) continua calculado só a partir das compras (método de custo médio ponderado): vender não altera o preço médio das unidades restantes, só reduz a quantidade — é o comportamento esperado nesse método contábil, então a query da seção 1.2/4.3 não precisou mudar.
- Cada card de "Minhas moedas" no dashboard ganhou um formulário de venda (quantidade, valor recebido, moeda), abaixo do link para o histórico. O histórico ([templates/asset_history.html](../templates/asset_history.html)) agora distingue "pago" (compra) de "recebido" (venda).

### 5.2 Frações menores que 0,01
- Os campos de quantidade e valor pago/recebido nos formulários de compra e venda tinham `min="0.0001"`/`min="0.01"` no HTML, o que bloqueava no navegador (antes mesmo de chegar ao servidor) valores bem pequenos — comuns em criptomoedas caras como Bitcoin (ex.: `0.00035` BTC, ou pagar/receber `35.00` já funcionava, mas quantidades ainda menores como `0.0001` ficavam no limite ou abaixo dele).
- Todos esses mínimos foram reduzidos para `0.00000001` (1e-8, precisão de "satoshi") em [templates/dashboard.html](../templates/dashboard.html). O banco nunca teve essa restrição — o `CHECK` em `movements` sempre foi só "maior que zero" — então o ajuste é puramente no HTML.
- Validado comprando `0.00035` Bitcoin pagando `122.50` em Real, e vendendo `0.0001` Bitcoin recebendo `35.00` em Real; saldo remanescente (`0.00025`) exibido corretamente no dashboard.

### 5.3 Verificações
- `cargo build`, `cargo clippy --all-targets` e `cargo fmt` (nos arquivos alterados) sem erros/avisos.
- `cargo test` com a role `invest_test`: 4/4 continuam passando (nenhum teste automatizado cobria compra/venda ainda — validação desta rodada foi manual, via `curl`).
- Fluxo manual: compra de fração pequena de Bitcoin, tentativa de venda maior que o saldo (`400`), venda parcial válida, histórico mostrando "pago"/"recebido" corretamente, quantidade líquida atualizada no dashboard.

## 6. Bug reportado — moeda recebida na venda não aparecia na carteira

Data: 2026-08-17

### 6.1 Causa

Uma venda (ex.: vender Bitcoin recebendo Dólar) gravava **apenas um** registro em `movements`: o do ativo vendido (Bitcoin), com `paid_currency_id`/`paid_amount` guardando só a *informação* de que a troca foi feita em Dólar — sem nunca criar um movimento correspondente **do lado do Dólar**. Como `list_user_holdings` calcula a posse de cada moeda somando os movimentos daquele `asset_id`, o Dólar nunca ganhava uma linha própria e por isso nunca aparecia em "Minhas moedas", mesmo tendo sido recebido na troca.

O mesmo problema existia (de forma menos visível) na compra: pagar por um ativo com outra moeda do catálogo não debitava essa moeda da carteira.

### 6.2 Correção

Toda compra/venda agora grava **dois movimentos, atomicamente, dentro de uma transação**:
1. O movimento do ativo negociado (como antes).
2. Um movimento **inverso** na moeda usada na troca — vender A recebendo B grava um "buy" de B (usando o ativo A como `paid_currency_id`/quantidade dele como `paid_amount`); comprar A pagando com C grava um "sell" de C.

Implementado em `Repository::create_trade` ([src/repository.rs](../src/repository.rs)), que abre uma transação (`self.db.begin()`), insere os dois movimentos e comita — ambos são gravados ou nenhum é, evitando um estado "pela metade" em caso de falha. `buy_asset`/`sell_asset` ([src/routes/frontend.rs](../src/routes/frontend.rs)) passaram a chamar `create_trade` em vez do antigo `create_movement` (removido).

O preço unitário (em reais) do movimento inverso usa a cotação atual da moeda envolvida (`Asset.unit_value`) — matematicamente é o mesmo valor que já era usado para calcular o preço em reais do ativo principal, então os dois lados da troca ficam consistentes entre si (nenhuma diferença de arredondamento entre o valor debitado de um lado e creditado do outro).

Efeito colateral desejado: como a moeda recebida numa venda passa a ter seu próprio saldo, ela também ganha seu próprio card com formulário de venda no dashboard — pode ser vendida ou usada para pagar novas compras normalmente.

Não há verificação de saldo suficiente da moeda *usada para pagar* numa compra (só do ativo vendido, numa venda) — de propósito: é assim que um usuário novo consegue comprar a primeira moeda sem já ter fundos cadastrados no sistema. Pagar com uma moeda que ainda não se possui deixa o saldo dela negativo, que simplesmente não aparece em "Minhas moedas" (a consulta já filtra saldo `> 0`). Ver seção "melhorias futuras" — considerar exigir saldo suficiente também do lado do pagamento, uma vez que o usuário tenha feito seu primeiro depósito/compra inicial.

### 6.3 Bug de exibição encontrado durante a correção

Ao validar o cenário reportado, o histórico de movimentação mostrava "pago: 0.00 Bitcoin" para uma venda de `0.0005` BTC — porque `movement.paid_amount` era formatado com 2 casas decimais (`fmt("{:.2}")`), adequado para valores em reais mas insuficiente para quantidades de criptomoeda. Corrigido em [templates/asset_history.html](../templates/asset_history.html) usando 8 casas decimais (mesma precisão já usada para `quantity`), evitando o truncamento para "0.00".

### 6.4 Validação

Cenário do relato reproduzido via `curl`: comprar `0.001` Bitcoin pagando em Real, depois vender `0.0005` Bitcoin recebendo `33.65` em Dólar Americano.
- Dashboard passou a mostrar **dois** cards em "Minhas moedas": Bitcoin (`0.0005` restante) **e** Dolar Americano (`33.65`, preço médio R$ 5,20 — igual à cotação usada na troca).
- Histórico do Dólar mostra a movimentação de "compra" com "pago: 0,00050000 Bitcoin" — refletindo corretamente a origem do saldo.
- `cargo build`, `cargo clippy --all-targets`, `cargo fmt` (arquivos alterados) e `cargo test` (role `invest_test`, 4/4) sem regressões.

## 7. Simplificação do dashboard — uma moeda, um card

Data: 2026-08-17

### 7.1 Problema

O dashboard tinha duplicação visual: a seção "Minhas moedas" mostrava um card (com link para o histórico e formulário de venda) para cada moeda que o usuário possuía, e mais abaixo as seções "Comprar criptomoedas"/"Comprar moedas fiduciárias" repetiam **a mesma moeda** num segundo card, só para comprar. Uma moeda que o usuário já possuía aparecia duas vezes na tela.

### 7.2 Solução

Cada moeda do catálogo passou a aparecer **uma única vez**, em `AssetOverview` (novo, em [src/routes/frontend.rs](../src/routes/frontend.rs)) — combina o ativo do catálogo com a posse do usuário (quantidade/preço médio ficam `0` quando ele nunca negociou aquela moeda, então essas linhas somem do card). O card de cada moeda tem:
- O **nome**, como link para `/assets/{id}` — clicar nele mostra o extrato de movimentações (rota já existente, inalterada).
- Dois botões, **comprar** e **vender**, implementados como `<details>`/`<summary>` do HTML — clicar expande o formulário correspondente (quantidade, valor, moeda de troca) sem precisar de JavaScript nenhum (mantém a mesma regra de "só o CDN do Tailwind como script").
- Para a moeda `Real` especificamente, um terceiro botão, **depositar** — ver seção 7.3.

As seções "Criptomoedas"/"Moedas fiduciárias" continuam existindo como agrupamento visual, mas agora cada uma lista o catálogo inteiro daquele tipo (não só o que o usuário possui), sempre um card por moeda.

### 7.3 Depósito (moeda Real)

Diferente de comprar/vender — que são sempre uma troca entre duas moedas do catálogo — um depósito é dinheiro entrando no sistema **sem** contrapartida em outro ativo (representa, por exemplo, uma transferência bancária externa). Por isso não reaproveita `create_trade`: `Repository::deposit` ([src/repository.rs](../src/repository.rs)) grava um único movimento de compra em que `paid_currency_id` referencia o próprio ativo (só faz sentido para a moeda-âncora, que vale sempre 1:1 em reais).

- Restrito à moeda chamada exatamente `"Real"` (constante `DEPOSITABLE_CURRENCY` em [src/routes/frontend.rs](../src/routes/frontend.rs)), validado tanto na exibição do botão quanto na rota `POST /assets/{id}/deposit` — tentar depositar em outra moeda retorna `400`.
- Simplificação assumida: a restrição é por **nome** (não existe uma coluna "é moeda-âncora" em `assets`); documentado aqui para o caso de precisar generalizar depois (ex.: um usuário cadastrar uma segunda moeda-âncora).
- Continua sem verificação de saldo suficiente do lado do pagamento em compras (seção 6.2) — o depósito é, na prática, a forma "oficial" de colocar a primeira Real na carteira, mas nada impede pagar uma compra com uma moeda ainda não depositada (fica negativa e simplesmente não aparece nos cards, já que a agregação filtra saldo `> 0`).

### 7.4 Validação

Fluxo completo testado via `curl` com um usuário novo:
- Dashboard inicial: cada moeda do catálogo aparece exatamente uma vez, com "comprar"/"vender"; somente `Real` tem "depositar".
- Depositar `1000` em Real → `303`. Depositar em Dólar → `400` (`"Invalid payment currency"`).
- Comprar `0.001` Bitcoin pagando `350` Real, depois vender `0.0004` Bitcoin recebendo `26` Real.
- Saldo final conferido: Real = `676,00` (`1000 − 350 + 26`), Bitcoin = `0,0006` (`0,001 − 0,0004`), preço médio do Bitcoin permanece `350000,00`.
- Extrato do Real (clicando no nome) mostra as três movimentações: depósito, débito da compra e crédito da venda.
- `cargo build`, `cargo clippy --all-targets`, `cargo fmt` (arquivos alterados) e `cargo test` (role `invest_test`, 4/4) sem regressões.
