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
//! ## O que este crate contém agora
//!
//! Além do manifesto, o [`RegistroModulos`] — coleta os [`Manifesto`]s compilados, resolve
//! o **grafo de dependências** (ordem topológica, ciclo = erro, conflitos) e calcula o
//! **conjunto efetivo** de uma empresa a partir da escolha do admin: módulos ativos,
//! submódulos (essenciais + pedidos, fechados sobre `depende_de`), permissões visíveis e o
//! menu já filtrado e ordenado (`docs/04-pilar-modularidade.md` §2.2 e §4).
//!
//! ## O que falta (ver `docs/17-roadmap.md`)
//!
//! A trait `Modulo` + o despacho de `Comando`/`Consulta` via `Ctx` sobre a
//! `UnidadeDeTrabalho` — a integração runtime com `cardeal-auth` (permissão) e
//! `cardeal-protocol`. As assinaturas estão em `docs/contratos-internos.md` §4.

#![forbid(unsafe_code)]
#![warn(missing_docs, clippy::pedantic)]
#![allow(clippy::module_name_repetitions, clippy::must_use_candidate)]

mod icone;
mod manifesto;
mod permissao;
mod registro;

pub use icone::Icone;
pub use manifesto::{ContaPadrao, EntradaMenu, ErroManifesto, IdModulo, Manifesto, Submodulo};
pub use permissao::{Permissao, Risco};
pub use registro::{ConjuntoEfetivo, ErroRegistro, PedidoAtivacao, RegistroModulos};
