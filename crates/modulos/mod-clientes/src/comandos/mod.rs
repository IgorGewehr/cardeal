//! Os comandos do módulo de clientes (`docs/modulos/clientes.md` §5). Um arquivo, um
//! comando, sempre nas cinco etapas de `docs/15-convencoes-codigo.md` §3: **carregar,
//! validar, lançar no razão, persistir, publicar** — este módulo nunca lança no razão
//! (`docs/modulos/clientes.md` §7), então a etapa 3 não existe aqui.
//!
//! Fatia mínima desta primeira versão: `CriarPessoa`, `EditarPessoa`, `AdicionarPapel`,
//! `DefinirLimiteCredito` — o suficiente para servir `os`/`vendas`/`compras`.
//! `ConsultarCadastroSefaz` (precisa de `PortaFiscal`), dedup/mesclagem e LGPD ficam para
//! quando tiverem consumidor (ver `src/lib.rs`).

mod adicionar_contato;
mod adicionar_endereco;
mod adicionar_papel;
mod criar_pessoa;
mod definir_limite_credito;
mod editar_pessoa;

pub use adicionar_contato::{AdicionarContato, ContatoFoiAdicionado};
pub use adicionar_endereco::{AdicionarEndereco, EnderecoFoiAdicionado};
pub use adicionar_papel::{AdicionarPapel, PapelFoiAdicionado};
pub use criar_pessoa::{ContatoInicial, CriarPessoa, EnderecoInicial, PessoaCadastrada};
pub use definir_limite_credito::{DefinirLimiteCredito, LimiteCreditoDefinido};
pub use editar_pessoa::{EditarPessoa, PessoaEditada};
