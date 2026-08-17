-- Execute como superusuário do PostgreSQL (ex.: role "postgres"):
--   psql "postgres://postgres:<senha>@localhost:5432/postgres" -f scripts/setup_test_role.sql
--
-- Cria uma role separada, só para rodar `cargo test`, com permissão de criar
-- bancos efêmeros (exigida pelo macro #[sqlx::test]). A role de produção
-- "invest" NÃO recebe essa permissão — ver docs/resumo-implementacao.md.

CREATE ROLE invest_test WITH LOGIN PASSWORD 'invest_test' CREATEDB;

CREATE DATABASE invest_test OWNER invest_test;
