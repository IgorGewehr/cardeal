//! Lançamento manual de nota de entrada — para comprar sem XML em mãos (fornecedor sem
//! NF-e, ou XML que ainda não chegou). `docs/modulos/compras.md` §5 só previa a entrada via
//! XML; esta é a mesma cascata de casamento/rateio de [`super::importar_nota_da_sefaz`], só
//! com os itens digitados em vez de extraídos de um XML — por isso **é** um `Comando` de
//! despacho de verdade (não depende de `PortaFiscal`).

use cardeal_kernel::{Cnpj, Data, Dinheiro, Erro, Id, Preco, Quantidade, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::nota::{EstadoCasamento, EstadoNotaEntrada, NotaEntrada};
use crate::preferencias::RateioPor;
use crate::repositorio::RepositorioCompras;

use super::{carregar_nota, confirmar_entrada_comum, resolver_fornecedor, RelatorioImportacao};
use crate::eventos::EntradaAConferir;

/// Um item digitado à mão — mesmos campos de um item de XML, sem a origem fiscal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemNotaManual {
    /// O código do produto no cadastro do fornecedor.
    pub codigo_fornecedor: String,
    /// A descrição, como o fornecedor chama o item.
    pub descricao: String,
    /// O NCM (8 dígitos) — usado na cascata de casamento por NCM + similaridade.
    pub ncm: String,
    /// A quantidade comprada.
    pub quantidade: Quantidade,
    /// O valor unitário cobrado.
    pub valor_unitario: Preco,
}

/// Lança uma nota de entrada sem XML: mesma cascata de casamento, mesmo rateio de despesas,
/// mesma confirmação automática quando a preferência permite — só a origem do dado muda.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LancarNotaManual {
    /// O CNPJ do fornecedor — resolve (ou cadastra) a pessoa, igual à entrada por XML.
    pub fornecedor_cnpj: String,
    /// A razão social do fornecedor, usada só se o cadastro ainda não existir.
    pub fornecedor_nome: String,
    /// Número da nota (ou do documento equivalente).
    pub numero: String,
    /// Série.
    pub serie: String,
    /// Data de emissão.
    pub data_emissao: Data,
    /// Os itens comprados.
    pub itens: Vec<ItemNotaManual>,
    /// Frete cobrado à parte.
    pub valor_frete: Dinheiro,
    /// Seguro cobrado à parte.
    pub valor_seguro: Dinheiro,
    /// Outras despesas acessórias.
    pub valor_outras_despesas: Dinheiro,
}

impl Comando for LancarNotaManual {
    type Saida = RelatorioImportacao;
    const PERMISSAO: &'static str = "compras.entrada.importar";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let cnpj = Cnpj::novo(&self.fornecedor_cnpj)?;
        let fornecedor = resolver_fornecedor(&cnpj, &self.fornecedor_nome, ctx, uow)?;
        let preferencias = RepositorioCompras::novo(uow).preferencias(ctx.empresa)?;

        let valor_produtos: Dinheiro = self
            .itens
            .iter()
            .map(|i| {
                Dinheiro::de_total(
                    i.quantidade,
                    i.valor_unitario,
                    cardeal_kernel::Arredondamento::MeioAcima,
                )
            })
            .sum();
        let despesas = self.valor_frete + self.valor_seguro + self.valor_outras_despesas;

        let nota = NotaEntrada {
            id: Id::novo(),
            empresa: ctx.empresa,
            fornecedor,
            chave_acesso: None,
            numero: self.numero,
            serie: self.serie,
            data_emissao: self.data_emissao,
            valor_produtos,
            valor_frete: self.valor_frete,
            valor_seguro: self.valor_seguro,
            valor_outras_despesas: self.valor_outras_despesas,
            valor_total: valor_produtos + despesas,
            estado: EstadoNotaEntrada::AConferir,
            versao: cardeal_kernel::Versao::INICIAL,
        };

        let pesos: Vec<i64> = match preferencias.rateio_por {
            RateioPor::Valor => self
                .itens
                .iter()
                .map(|i| {
                    Dinheiro::de_total(
                        i.quantidade,
                        i.valor_unitario,
                        cardeal_kernel::Arredondamento::MeioAcima,
                    )
                    .em_centavos()
                })
                .collect(),
            RateioPor::Peso => self
                .itens
                .iter()
                .map(|i| i.quantidade.unidades_internas())
                .collect(),
        };
        let itens = super::montar_itens(
            nota.id,
            fornecedor,
            despesas,
            &pesos,
            self.itens.iter().map(|i| {
                (
                    i.codigo_fornecedor.as_str(),
                    i.descricao.as_str(),
                    i.ncm.as_str(),
                    i.quantidade,
                    i.valor_unitario,
                )
            }),
            ctx,
            uow,
        )?;

        {
            let mut repo = RepositorioCompras::novo(uow);
            repo.inserir_nota(&nota)?;
            for item in &itens {
                repo.inserir_item(item)?;
            }
        }

        let itens_nao_casados = itens
            .iter()
            .filter(|i| i.estado_casamento != EstadoCasamento::Casado)
            .count();

        if preferencias.confirma_automaticamente_quando_tudo_casa && itens_nao_casados == 0 {
            if let Some(local) = preferencias.local_padrao {
                confirmar_entrada_comum(
                    nota.id,
                    local,
                    preferencias.gera_titulo_a_pagar,
                    ctx,
                    uow,
                )?;
                let confirmada = carregar_nota(uow, nota.id)?;
                return Ok(RelatorioImportacao {
                    nota_entrada: nota.id,
                    estado: confirmada.estado,
                    itens_nao_casados: 0,
                });
            }
        }

        uow.publicar(EntradaAConferir {
            nota_entrada: nota.id,
            fornecedor,
            total_itens: itens.len(),
        })
        .map_err(|e| Erro::de_dominio(&e))?;

        Ok(RelatorioImportacao {
            nota_entrada: nota.id,
            estado: EstadoNotaEntrada::AConferir,
            itens_nao_casados,
        })
    }
}
