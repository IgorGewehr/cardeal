//! # cardeal-pdf
//!
//! Geração dos documentos PDF do Cardeal. Hoje: o **orçamento profissional** — cabeçalho com
//! a logo e os dados da empresa, tabela de itens paginada, quadro de totais, condições
//! comerciais e rodapé de aceite.
//!
//! O escritor de PDF é próprio ([`documento`]) — PDF 1.7 mínimo com as fontes padrão
//! (Helvetica / Helvetica-Bold, `WinAnsiEncoding`), sem nenhuma biblioteca de PDF ou de
//! compressão. Só `image` entra, para decodificar a logo.
//!
//! ```no_run
//! use cardeal_pdf::{gerar_orcamento, IdentidadeEmpresa, OrcamentoPdf};
//! # fn exemplo(empresa: IdentidadeEmpresa, orc: OrcamentoPdf) -> Result<(), cardeal_pdf::ErroPdf> {
//! let bytes = gerar_orcamento(&empresa, &orc, cardeal_kernel::Instante::agora())?;
//! std::fs::write("orcamento.pdf", bytes).unwrap();
//! # Ok(()) }
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs, clippy::pedantic)]
#![allow(clippy::module_name_repetitions, clippy::must_use_candidate)]
// Um crate de layout e de serialização de formato binário: converter coordenadas em `f32`,
// componentes de cor em byte e tamanhos de objeto em texto é o trabalho, não um descuido.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::return_self_not_must_use
)]

mod documento;
mod fonte;
mod imagem;
mod layout;

use cardeal_kernel::{Data, Dinheiro, Instante, Percentual, Preco, Quantidade};

/// O que pode falhar ao gerar um PDF.
#[derive(Debug, thiserror::Error)]
pub enum ErroPdf {
    /// A logo cadastrada não é uma imagem PNG/JPEG válida.
    #[error("logo inválida: {0}")]
    Logo(String),
}

/// A identidade da empresa que aparece no cabeçalho e no rodapé do documento.
#[derive(Debug, Clone, Default)]
pub struct IdentidadeEmpresa {
    /// Nome fantasia — o nome grande do cabeçalho.
    pub nome_fantasia: String,
    /// Razão social — a linha discreta abaixo do nome fantasia.
    pub razao_social: String,
    /// CNPJ já formatado (`00.000.000/0000-00`).
    pub cnpj: String,
    /// Endereço em uma linha.
    pub endereco: String,
    /// Telefone.
    pub telefone: String,
    /// E-mail.
    pub email: String,
    /// Site.
    pub site: String,
    /// Bytes de um PNG/JPEG com a logo, se houver.
    pub logo_png: Option<Vec<u8>>,
}

/// Uma linha da tabela de itens.
#[derive(Debug, Clone)]
pub struct ItemPdf {
    /// Descrição livre (serviço, peça, etapa…).
    pub descricao: String,
    /// Quantidade.
    pub quantidade: Quantidade,
    /// Unidade (`un`, `h`, `m²`…). Pode ser vazia.
    pub unidade: String,
    /// Preço unitário.
    pub preco_unitario: Preco,
    /// Desconto do item em pontos percentuais.
    pub desconto_pct: Percentual,
    /// Total da linha, já com o desconto aplicado.
    pub total: Dinheiro,
}

/// Os dados de um orçamento a imprimir.
#[derive(Debug, Clone)]
pub struct OrcamentoPdf {
    /// Número sequencial.
    pub numero: u64,
    /// Nome do cliente.
    pub cliente_nome: String,
    /// Documento do cliente (pode ser vazio).
    pub cliente_documento: String,
    /// Contato do cliente — telefone/e-mail (pode ser vazio).
    pub cliente_contato: String,
    /// Assunto / título do trabalho.
    pub assunto: String,
    /// Texto de apresentação/escopo (pode ser vazio).
    pub descricao: String,
    /// Data de emissão.
    pub data_emissao: Data,
    /// Validade da proposta.
    pub validade: Data,
    /// Condições de pagamento (pode ser vazio).
    pub condicoes_pagamento: String,
    /// Prazo de entrega/execução (pode ser vazio).
    pub prazo_entrega: String,
    /// Observações gerais (pode ser vazio).
    pub observacoes: String,
    /// Nome de quem emitiu — vai no rodapé (pode ser vazio).
    pub responsavel: String,
    /// As linhas.
    pub itens: Vec<ItemPdf>,
    /// Soma dos itens antes do desconto.
    pub subtotal: Dinheiro,
    /// Desconto total (soma dos descontos de item).
    pub desconto: Dinheiro,
    /// Total líquido.
    pub total: Dinheiro,
}

/// Gera os bytes do PDF de um orçamento.
///
/// # Errors
/// [`ErroPdf::Logo`] se `empresa.logo_png` não decodificar.
pub fn gerar_orcamento(
    empresa: &IdentidadeEmpresa,
    orcamento: &OrcamentoPdf,
    gerado_em: Instante,
) -> Result<Vec<u8>, ErroPdf> {
    layout::gerar(empresa, orcamento, gerado_em)
}
