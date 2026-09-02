//! # cardeal-modkit
//!
//! O sistema de módulos do Cardeal. Ver `docs/04-pilar-modularidade.md` para a tese: um
//! único binário atende MEI e posto de combustível porque o que cada empresa vê é
//! **derivado em runtime** de um registro declarativo — nunca escrito à mão por tela.
//!
//! ## O que este crate contém agora
//!
//! O **manifesto**: [`Manifesto`], [`Submodulo`], [`Permissao`], [`EntradaMenu`],
//! [`ContaPadrao`], [`IdModulo`], [`Icone`] — os dados estáticos com que cada módulo declara
//! sua identidade, dependências, permissões e contribuições de menu.
//! [`Manifesto::validar`] prova a consistência interna de um manifesto (submódulos únicos,
//! permissões dentro do namespace do módulo, menu sem referência solta) sem tocar em banco.
//!
//! ## O que falta (ver `docs/17-roadmap.md`)
//!
//! O `Registro` (coleta os manifestos de todos os módulos compilados, resolve o grafo de
//! dependências entre eles, calcula o conjunto efetivo por perfil/tenant) e o despacho de
//! comandos/consultas via `Ctx`/`UnidadeDeTrabalho` — ambos dependem de `cardeal-storage`,
//! que ainda não existe. As assinaturas exatas estão em `docs/contratos-internos.md` §4.

#![forbid(unsafe_code)]
#![warn(missing_docs, clippy::pedantic)]
#![allow(clippy::module_name_repetitions, clippy::must_use_candidate)]

mod icone;
mod manifesto;
mod permissao;

pub use icone::Icone;
pub use manifesto::{ContaPadrao, EntradaMenu, ErroManifesto, IdModulo, Manifesto, Submodulo};
pub use permissao::{Permissao, Risco};
