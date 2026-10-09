//! Aging (idade) das parcelas em aberto e inadimplência: quanto está a vencer, quanto está
//! atrasado e há quanto tempo, e quem deve mais. Serve a receber (quem me deve) e a pagar
//! (a quem devo). Domínio puro sobre as parcelas já lidas.

use cardeal_kernel::{Data, Dinheiro, Percentual};
use cardeal_ledger::Contraparte;
use serde::{Deserialize, Serialize};

use crate::consultas::ItemTituloEmAberto;

/// Uma faixa de atraso.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FaixaAging {
    /// "A vencer", "1 a 30 dias"…
    pub rotulo: String,
    /// Saldo em aberto na faixa.
    pub valor: Dinheiro,
    /// Quantas parcelas.
    pub parcelas: u32,
}

/// Quem deve (ou a quem se deve), com o atraso mais antigo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Devedor {
    /// A contraparte; `None` = títulos avulsos sem pessoa.
    pub contraparte: Option<Contraparte>,
    /// O saldo já vencido.
    pub vencido: Dinheiro,
    /// O saldo ainda a vencer.
    pub a_vencer: Dinheiro,
    /// Dias de atraso da parcela vencida mais antiga (0 = nada vencido).
    pub dias_do_mais_antigo: u32,
    /// Parcelas vencidas.
    pub parcelas_vencidas: u32,
}

/// O resumo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Aging {
    /// A data de referência.
    pub em: Data,
    /// As faixas: a vencer, 1–30, 31–60, 61–90 e mais de 90 dias.
    pub faixas: Vec<FaixaAging>,
    /// Todo o saldo em aberto.
    pub total_aberto: Dinheiro,
    /// O que já venceu.
    pub total_vencido: Dinheiro,
    /// O que venceu há mais de 30 dias.
    pub vencido_mais_de_30: Dinheiro,
    /// Quem deve, do maior vencido para o menor; só quem tem algo vencido.
    pub devedores: Vec<Devedor>,
}

impl Aging {
    /// Taxa de inadimplência: o vencido há mais de 30 dias sobre o total em aberto.
    #[must_use]
    pub fn taxa_de_inadimplencia(&self) -> Option<Percentual> {
        (self.total_aberto.e_positivo()).then(|| {
            Percentual::da_razao(
                self.vencido_mais_de_30.em_centavos(),
                self.total_aberto.em_centavos(),
            )
        })
    }
}

/// Monta o aging a partir das parcelas em aberto.
#[must_use]
pub fn montar(parcelas: &[ItemTituloEmAberto], hoje: Data) -> Aging {
    const ROTULOS: [&str; 5] = [
        "A vencer",
        "1 a 30 dias",
        "31 a 60 dias",
        "61 a 90 dias",
        "Mais de 90 dias",
    ];
    let mut faixas: Vec<FaixaAging> = ROTULOS
        .iter()
        .map(|r| FaixaAging {
            rotulo: (*r).to_owned(),
            valor: Dinheiro::ZERO,
            parcelas: 0,
        })
        .collect();
    let mut total_aberto = Dinheiro::ZERO;
    let mut total_vencido = Dinheiro::ZERO;
    let mut vencido_mais_de_30 = Dinheiro::ZERO;
    let mut devedores: Vec<Devedor> = Vec::new();

    for p in parcelas {
        let saldo = p.saldo();
        if !saldo.e_positivo() {
            continue;
        }
        let atraso = hoje.dias_ate(p.vencimento).saturating_neg();
        let faixa = match atraso {
            i32::MIN..=0 => 0,
            1..=30 => 1,
            31..=60 => 2,
            61..=90 => 3,
            _ => 4,
        };
        faixas[faixa].valor += saldo;
        faixas[faixa].parcelas += 1;
        total_aberto += saldo;
        if atraso > 0 {
            total_vencido += saldo;
        }
        if atraso > 30 {
            vencido_mais_de_30 += saldo;
        }

        let pos = devedores
            .iter()
            .position(|d| match (&d.contraparte, &p.contraparte) {
                (Some(a), Some(b)) => a.id() == b.id(),
                (None, None) => true,
                _ => false,
            });
        let i = pos.unwrap_or_else(|| {
            devedores.push(Devedor {
                contraparte: p.contraparte,
                vencido: Dinheiro::ZERO,
                a_vencer: Dinheiro::ZERO,
                dias_do_mais_antigo: 0,
                parcelas_vencidas: 0,
            });
            devedores.len() - 1
        });
        let d = &mut devedores[i];
        if atraso > 0 {
            d.vencido += saldo;
            d.parcelas_vencidas += 1;
            d.dias_do_mais_antigo = d
                .dias_do_mais_antigo
                .max(u32::try_from(atraso).unwrap_or(0));
        } else {
            d.a_vencer += saldo;
        }
    }
    devedores.retain(|d| d.vencido.e_positivo());
    devedores.sort_by_key(|d| std::cmp::Reverse(d.vencido));

    Aging {
        em: hoje,
        faixas,
        total_aberto,
        total_vencido,
        vencido_mais_de_30,
        devedores,
    }
}

#[cfg(test)]
mod testes {
    use cardeal_kernel::Id;

    use super::*;
    use crate::titulo::{EspecieTitulo, EstadoParcela};

    fn hoje() -> Data {
        Data::de_ymd(2026, 10, 9).unwrap()
    }

    fn parcela(
        cliente: Option<Id>,
        dias_de_atraso: i32,
        valor: i64,
        baixado: i64,
    ) -> ItemTituloEmAberto {
        ItemTituloEmAberto {
            parcela: Id::novo(),
            titulo: Id::novo(),
            numero: 1,
            especie: EspecieTitulo::Receber,
            contraparte: cliente.map(Contraparte::Cliente),
            vencimento: hoje().mais_dias(-dias_de_atraso),
            valor_original: Dinheiro::reais(valor),
            valor_baixado: Dinheiro::reais(baixado),
            estado: EstadoParcela::Aberta,
            descricao: None,
            origem_modulo: "financeiro".into(),
            origem_id: None,
            categoria: None,
        }
    }

    #[test]
    fn faixas_somam_o_saldo_e_a_inadimplencia_conta_so_o_que_passa_de_30_dias() {
        let (ana, beto) = (Id::novo(), Id::novo());
        let ps = vec![
            parcela(Some(ana), -5, 100, 0),  // a vencer em 5 dias
            parcela(Some(ana), 10, 200, 50), // 10 dias, saldo 150
            parcela(Some(beto), 45, 300, 0), // 31–60
            parcela(Some(beto), 120, 400, 0),
            parcela(None, 0, 50, 0), // vence hoje: ainda não está atrasada
        ];
        let a = montar(&ps, hoje());
        let v: Vec<i64> = a
            .faixas
            .iter()
            .map(|f| f.valor.em_centavos() / 100)
            .collect();
        assert_eq!(v, [150, 150, 300, 0, 400]);
        assert_eq!(a.total_aberto, Dinheiro::reais(1_000));
        assert_eq!(a.total_vencido, Dinheiro::reais(850));
        assert_eq!(a.vencido_mais_de_30, Dinheiro::reais(700));
        assert_eq!(a.taxa_de_inadimplencia().unwrap().formatar(0), "70");
        // Beto deve mais e há mais tempo; quem só tem a vencer nem aparece.
        assert_eq!(a.devedores.len(), 2);
        assert_eq!(a.devedores[0].contraparte, Some(Contraparte::Cliente(beto)));
        assert_eq!(a.devedores[0].vencido, Dinheiro::reais(700));
        assert_eq!(a.devedores[0].dias_do_mais_antigo, 120);
        assert_eq!(a.devedores[1].a_vencer, Dinheiro::reais(100));
        assert_eq!(a.devedores[1].parcelas_vencidas, 1);
    }

    #[test]
    fn parcela_sem_saldo_e_lista_vazia_nao_quebram() {
        assert_eq!(montar(&[], hoje()).taxa_de_inadimplencia(), None);
        let a = montar(&[parcela(None, 10, 100, 100)], hoje());
        assert_eq!(a.total_aberto, Dinheiro::ZERO);
        assert!(a.devedores.is_empty());
    }
}

/// O aging do que os clientes devem, na data de hoje.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct AgingAReceber;

/// O aging do que a empresa deve, na data de hoje.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct AgingAPagar;

#[cfg(feature = "sqlite")]
fn apurar(
    ctx: &cardeal_modkit::Ctx,
    conexao: &rusqlite::Connection,
    especie: crate::titulo::EspecieTitulo,
) -> cardeal_kernel::Resultado<Aging> {
    let parcelas = crate::consultas::titulos_em_aberto(conexao, ctx.empresa, especie)?;
    Ok(montar(&parcelas, ctx.hoje()))
}

impl cardeal_modkit::Consulta for AgingAReceber {
    type Saida = Aging;
    const PERMISSAO: &'static str = "financeiro.receber.ver";

    #[cfg(feature = "sqlite")]
    fn executar(
        self,
        ctx: &cardeal_modkit::Ctx,
        conexao: &rusqlite::Connection,
    ) -> cardeal_kernel::Resultado<Self::Saida> {
        apurar(ctx, conexao, crate::titulo::EspecieTitulo::Receber)
    }
}

impl cardeal_modkit::Consulta for AgingAPagar {
    type Saida = Aging;
    const PERMISSAO: &'static str = "financeiro.pagar.ver";

    #[cfg(feature = "sqlite")]
    fn executar(
        self,
        ctx: &cardeal_modkit::Ctx,
        conexao: &rusqlite::Connection,
    ) -> cardeal_kernel::Resultado<Self::Saida> {
        apurar(ctx, conexao, crate::titulo::EspecieTitulo::Pagar)
    }
}
