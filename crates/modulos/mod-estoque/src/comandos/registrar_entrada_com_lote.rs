//! Registra uma entrada de estoque que também cria um lote/peça rastreável com código de
//! post-it — comando **próprio**, separado de [`super::RegistrarEntrada`], para não quebrar
//! quem já constrói `RegistrarEntrada` sem saber de lote (telas e testes existentes).
//!
//! `docs/modulos/estoque.md` §3 e §5 — pedido do dono da assistência técnica: peça chega,
//! alguém escreve um código curto num post-it, cola na peça. Este comando é o que grava essa
//! entrada com o código, a origem (compra ou aparelho usado desmontado) e o custo específico
//! desta peça.

use cardeal_kernel::{Data, Id, Preco, Quantidade, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::comandos::{registrar_entrada_com_lote_comum, DadosEntrada, DadosNovoLote};
use crate::produto::OrigemLote;

/// Registra uma entrada de estoque criando também um lote/peça rastreável (código de
/// post-it).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistrarEntradaComLote {
    /// O produto.
    pub produto: Id,
    /// O local que recebe.
    pub local: Id,
    /// A quantidade (normalmente 1, para uma peça individual; pode ser maior para um lote de
    /// peças idênticas da mesma origem/custo).
    pub quantidade: Quantidade,
    /// O custo unitário desta entrada — o custo real desta peça específica, que pode
    /// divergir do custo médio do produto.
    pub custo_unitario: Preco,
    /// O código curto e digitável (o texto do post-it), único por empresa.
    pub codigo_lote: String,
    /// De onde a peça veio: compra nova ou aparelho usado desmontado.
    pub origem: OrigemLote,
    /// O fornecedor, quando `origem = Compra`.
    pub fornecedor: Option<Id>,
    /// O aparelho usado de origem — obrigatório quando `origem = AparelhoUsado`
    /// (`RegistrarAparelhoOrigem` cria esse registro antes).
    pub aparelho_origem: Option<Id>,
    /// Data de fabricação, quando conhecida.
    pub fabricacao: Option<Data>,
    /// Data de validade, quando o produto controla validade.
    pub validade: Option<Data>,
}

/// O que o comando devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct EntradaComLoteRegistrada {
    /// O movimento criado.
    pub movimento: Id,
    /// O custo médio do produto resultante após a entrada.
    pub custo_medio: Preco,
    /// O lote/peça rastreável criado — o id a associar ao código no post-it.
    pub lote: Id,
}

impl Comando for RegistrarEntradaComLote {
    type Saida = EntradaComLoteRegistrada;
    const PERMISSAO: &'static str = "estoque.movimento.entrada_com_lote";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let g = registrar_entrada_com_lote_comum(
            DadosEntrada {
                produto: self.produto,
                local: self.local,
                quantidade: self.quantidade,
                custo_unitario: self.custo_unitario,
                origem_modulo: "estoque",
                origem_id: None,
            },
            DadosNovoLote {
                codigo: self.codigo_lote,
                origem: self.origem,
                fornecedor: self.fornecedor,
                aparelho_origem: self.aparelho_origem,
                fabricacao: self.fabricacao,
                validade: self.validade,
            },
            ctx,
            uow,
        )?;
        Ok(EntradaComLoteRegistrada {
            movimento: g.movimento,
            custo_medio: g.custo_medio,
            lote: g
                .lote
                .expect("registrar_entrada_com_lote_comum sempre devolve um lote"),
        })
    }
}
