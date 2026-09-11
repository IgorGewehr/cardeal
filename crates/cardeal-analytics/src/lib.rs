//! cardeal-analytics — hoje, a lucratividade real de OS (`docs/17-roadmap.md` Fase 4 chegou
//! antes, a pedido do usuário: "onde está entrando dinheiro ou não").
//!
//! ## O que este crate contém
//!
//! [`margem`]: domínio puro de margem — receita, custo real de peça, tempo apontado e (onde
//! configurável) custo de mão de obra, calculados a partir das consultas públicas de
//! `mod-os` (nunca lendo `os_*` direto — `docs/contratos-internos.md` §7 regra 2).
//! [`margem::calcular_margem_de_os`] dá a lucratividade de uma ordem; [`margem::agregar_margens`]
//! soma um conjunto (ex.: todas as OS faturadas num mês) para um dashboard futuro.
//!
//! Sem UI aqui — a tela (`cardeal-desktop`) consome este crate diretamente.
//!
//! ## O que falta (ver `docs/17-roadmap.md`, Fase 4)
//!
//! O projetor colunar (Parquet/DuckDB) que dá nome ao crate, curva ABC, previsão de fluxo de
//! caixa/demanda e radar de anomalias — tudo isso segue como está descrito no roadmap. O que
//! foi implementado agora é o primeiro caso de uso concreto que o usuário pediu.
#![forbid(unsafe_code)]
#![warn(missing_docs, clippy::pedantic)]
#![allow(clippy::module_name_repetitions, clippy::must_use_candidate)]

pub mod margem;

pub use margem::{
    agregar_margens, calcular_margem_de_os, CustoHorario, MargemAgregada, MargemDeOs,
};
