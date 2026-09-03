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
//! Além disso, o **despacho** ([`despacho`]): os traits [`Modulo`], [`Comando`] e
//! [`Consulta`], o [`Registro`] onde um módulo declara seus manipuladores, o [`Ctx`] de
//! execução e o [`Despachante`] que resolve módulo ativo + autorização
//! ([`cardeal_auth::autorizar`]) e roda o manipulador dentro da transação do
//! `Escritor` (comando) ou sobre o `Leitor` (consulta).
//!
//! ## O que falta (ver `docs/17-roadmap.md`)
//!
//! `Ctx::contas()` (espera um resolvedor não-genérico em `cardeal-ledger`), o [`Registro`]
//! para assinaturas de evento, tarefas agendadas, portas e itens do Pulso, o replay de
//! idempotência e a auditoria automática de `Comando::AUDITA`, e a macro `#[comando(...)]`
//! (virá com `cardeal-protocol`). As assinaturas estão em `docs/contratos-internos.md` §4.

#![forbid(unsafe_code)]
#![warn(missing_docs, clippy::pedantic)]
#![allow(clippy::module_name_repetitions, clippy::must_use_candidate)]
#![allow(clippy::result_large_err)] // `Erro`/`ErroArmazenamento` são grandes de propósito (detalhes ao usuário)

mod despacho;
mod icone;
mod manifesto;
mod permissao;
mod registro;

pub use despacho::{Ambiente, Comando, Consulta, Ctx, Despachante, Modulo, Registro};
pub use icone::Icone;
pub use manifesto::{ContaPadrao, EntradaMenu, ErroManifesto, IdModulo, Manifesto, Submodulo};
pub use permissao::{Permissao, Risco};
pub use registro::{ConjuntoEfetivo, ErroRegistro, PedidoAtivacao, RegistroModulos};
