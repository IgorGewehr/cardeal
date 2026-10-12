//! Análise sobre o que os módulos já registraram — domínio puro, sem I/O e sem `f64`.
//!
//! Os módulos expõem consultas públicas; quem orquestra (a tela, hoje) junta as respostas e
//! chama as funções daqui. Este crate nunca lê tabela nenhuma.

pub mod rentabilidade;

pub use rentabilidade::{
    calcular, CustoDoProduto, NoDaEstrutura, NoDeRentabilidade, ProdutoDaEstrutura,
    ProdutoRentavel, ReceitaDoProduto, Rentabilidade, Totais,
};
