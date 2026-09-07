//! # mod-clientes
//!
//! O cadastro único de pessoas do Cardeal — cliente, fornecedor, transportadora,
//! funcionário, sócio e vendedor são **papéis da mesma pessoa**, nunca cadastros separados.
//! Ver `docs/modulos/clientes.md` para a especificação funcional completa.
//!
//! ## O que este crate contém agora
//!
//! O **domínio puro** (`docs/19-estado-e-processo.md` §3.2):
//!
//! - [`Pessoa`] / [`Papel`] / [`ConstrutorPessoa`] — cadastro, papéis que coexistem,
//!   máquina de estado `Ativa → Inativa → Anonimizada` e a **anonimização LGPD** que
//!   preserva o `id` (e portanto os títulos e lançamentos).
//! - [`DocumentoPessoa`] / [`Endereco`] / [`Contato`] — validação de CPF/CNPJ por dígito
//!   verificador **sem I/O** (§11.8), UF, CEP e formato de contato.
//! - [`LimiteCredito`] / [`Score`] — bloqueio automático, liberação sempre manual e
//!   auditada (§11.4), score **derivado, nunca editável** (§11.5).
//! - [`dedup`] — sugestão de duplicidade por documento (certeza) e por similaridade de
//!   nome (nunca bloqueia — §11.1).
//! - [`manifesto`] — a identidade declarativa do módulo, validada.
//!
//! E a **amarração ao motor** ([`ModuloClientes`]): manifesto, migrações
//! (`clientes_pessoa`/`clientes_papel`/`clientes_documento`/`clientes_limite_credito`, e
//! desde a v2 `clientes_contato`/`clientes_endereco`) e os comandos **[`CriarPessoa`]**,
//! **[`EditarPessoa`]**, **[`AdicionarPapel`]**, **[`DefinirLimiteCredito`]**,
//! **[`AdicionarContato`]** e **[`AdicionarEndereco`]** — a fatia mínima que destrava
//! `os`/`vendas`/`compras` (todos referenciam `clientes_pessoa.id` como contraparte de
//! título e cliente/fornecedor) mais o mínimo pra realmente conseguir ligar para o cliente
//! (auditoria de produção encontrou essa lacuna: `Contato`/`Endereco` existiam no domínio
//! desde o início, mas nenhum comando os gravava).
//!
//! ## O que falta (ver `docs/17-roadmap.md`, Fase 1)
//!
//! `ConsultarCadastroSefaz` (precisa de `PortaFiscal`, que ainda não existe),
//! `MesclarPessoas`/`SugerirMesclagem` (dedup), `AnonimizarPessoa`/`ExportarDadosDoTitular`
//! (LGPD), `VincularVendedor`/`VincularTabelaPreco`, `BloquearCredito`/`LiberarCredito`, e
//! as **consultas** paginadas (`BuscarPessoa`, `LimiteDisponivel`…) — nenhuma bloqueia o
//! módulo `os`. Este módulo **não lança dinheiro** — não há receituário
//! (`docs/modulos/clientes.md` §7).

#![forbid(unsafe_code)]
#![warn(missing_docs, clippy::pedantic)]
#![allow(clippy::module_name_repetitions, clippy::must_use_candidate)]
#![allow(clippy::result_large_err)]

mod cadastro;
mod comandos;
mod consultas;
mod credito;
pub mod dedup;
mod erros;
pub mod eventos;
mod manifesto;
pub mod migracoes;
mod modulo;
mod pessoa;
mod repositorio;

pub use cadastro::{Contato, DocumentoPessoa, Endereco, TipoContato, TipoDocumento, TipoEndereco};
pub use comandos::{
    AdicionarContato, AdicionarEndereco, AdicionarPapel, ContatoFoiAdicionado, ContatoInicial,
    CriarPessoa, DefinirLimiteCredito, EditarPessoa, EnderecoFoiAdicionado, EnderecoInicial,
    LimiteCreditoDefinido, PapelFoiAdicionado, PessoaCadastrada, PessoaEditada,
};
pub use consultas::{DetalhePessoa, ItemPessoa, PessoaDetalhada, PessoasPorPapel};
pub use credito::{
    DisponivelCredito, EventoCredito, LimiteCredito, Score, SituacaoCredito, SCORE_INICIAL,
    SCORE_MAXIMO, SCORE_MINIMO,
};
pub use erros::ErroClientes;
pub use manifesto::{manifesto, MANIFESTO};
pub use modulo::ModuloClientes;
pub use pessoa::{ConstrutorPessoa, EstadoPessoa, Papel, PapelPessoa, Pessoa, TipoPessoa};
pub use repositorio::RepositorioClientes;
