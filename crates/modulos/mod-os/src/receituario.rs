//! O receituário contábil da OS: o faturamento vira um único lançamento combinando receita
//! de serviço e o custo das peças aplicadas.
//!
//! `docs/modulos/os.md` §7. Esta função só **monta** o
//! [`LancamentoBalanceado`](cardeal_ledger::LancamentoBalanceado); quem o grava é o comando,
//! via [`Razao::registrar`](cardeal_ledger::Razao::registrar). Peça coberta por garantia
//! nunca chega aqui com custo (ver `crate::execucao::ItemPeca::total_custo`) — esta versão
//! não gera receita nem custo diferenciados por garantia porque `coberto_garantia` ainda
//! não é ligado por nenhum comando (`AcionarGarantia` fica para depois).

use cardeal_kernel::{Data, Dinheiro, Fuso, Id, Instante};
use cardeal_ledger::{ConstrutorLancamento, Contraparte, LancamentoBalanceado, Origem};

use crate::ordem::OrdemServico;

/// Quem está postando, de onde e quando.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Autoria {
    /// O usuário responsável.
    pub usuario: Id,
    /// O dispositivo de origem.
    pub dispositivo: Id,
    /// O instante da unidade de trabalho.
    pub agora: Instante,
    /// O fuso da empresa.
    pub fuso: Fuso,
}

impl Autoria {
    /// A data corrente, no fuso da empresa.
    #[must_use]
    pub const fn hoje(&self) -> Data {
        self.agora.data(self.fuso)
    }
}

/// As contas que o faturamento de uma OS usa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContasFaturamento {
    /// Clientes a receber (1.2.01).
    pub clientes_a_receber: Id,
    /// Receita de serviços (4.2).
    pub receita_servicos: Id,
    /// Custo de serviço (5.2) — só debitado se houver custo de peça aplicada.
    pub custo_servico: Id,
    /// Estoque de mercadorias (1.3.01) — só creditado junto com o custo de serviço.
    pub estoque: Id,
}

fn origem_os(ordem_servico: Id) -> Origem {
    Origem::modulo("os")
        .tipo("ordem_servico")
        .agregado(ordem_servico)
}

/// Monta o lançamento `Confirmado` de faturamento: D Clientes a receber / C Receita de
/// serviços pelo total cobrado (pulado se `os.valor_total` for zero — um reparo em garantia
/// não gera receita nem conta a receber), e — na mesma partida dobrada, se houver custo de
/// peça aplicada — D Custo de serviço / C Estoque. `None` quando não há **nada** a lançar
/// (nem cobrança nem custo de peça — um serviço cortesia, sem peça nenhuma): o `Razao` nunca
/// aceita um lançamento vazio, e não há nada de financeiro para registrar mesmo.
///
/// # Panics
/// Nunca, na prática, quando devolve `Some`: débito e crédito usam sempre os mesmos totais
/// (`os.valor_total`/`total_custo_pecas` de cada lado), então o `ConstrutorLancamento` não
/// pode desbalancear — mesma garantia de `mod_financeiro`'s `lancamento_dc`.
#[must_use]
pub fn faturar_ordem_servico(
    os: &OrdemServico,
    total_custo_pecas: Dinheiro,
    contas: ContasFaturamento,
    autoria: Autoria,
) -> Option<LancamentoBalanceado> {
    if !os.valor_total.e_positivo() && !total_custo_pecas.e_positivo() {
        return None;
    }
    let historico = format!("Ordem de serviço #{} faturada", os.numero);
    let mut c = ConstrutorLancamento::novo(os.empresa, autoria.hoje(), historico)
        .origem(origem_os(os.id))
        .criado_por(autoria.usuario, autoria.dispositivo)
        .agora(autoria.agora);
    if os.valor_total.e_positivo() {
        c = c
            .debitar(contas.clientes_a_receber, os.valor_total)
            .contraparte(Contraparte::Cliente(os.cliente))
            .creditar(contas.receita_servicos, os.valor_total);
    }
    c = c
        .debitar_se(contas.custo_servico, total_custo_pecas)
        .creditar_se(contas.estoque, total_custo_pecas);
    Some(
        c.construir()
            .expect("débito e crédito usam os mesmos totais dos dois lados, sempre balanceia"),
    )
}
