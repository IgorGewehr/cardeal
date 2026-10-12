//! O aviso depois de confirmar uma compra: o custo mudou, então algum preço da tabela de venda
//! pode ter ficado para trás. Só lê (`vendas.precos_defasados.v1`); sem tabela, sem permissão
//! ou com qualquer erro, não avisa nada — a compra já deu certo e o aviso é só um lembrete.

use cardeal_cliente::{Motor, Sessao};
use mod_vendas::{PrecoDefasado, PrecosDefasados, TabelaPreco, TabelasDePreco, TipoTabela};

/// O texto do aviso, ou `None` quando nada ficou defasado.
#[must_use]
pub fn texto_do_aviso(defasados: usize) -> Option<String> {
    match defasados {
        0 => None,
        n => Some(format!(
            "{n} preço(s) ficaram defasados com esta compra — veja Estoque › Preços"
        )),
    }
}

/// Quantos preços da primeira tabela de venda ativa estão em prejuízo ou abaixo do alvo.
#[must_use]
pub fn contar_defasados(motor: &Motor, sessao: &Sessao) -> usize {
    let Ok(tabelas) =
        motor.consultar::<TabelasDePreco>(sessao, "vendas.tabelas_de_preco.v1", &TabelasDePreco)
    else {
        return 0;
    };
    let tabelas: Vec<TabelaPreco> = tabelas;
    let Some(tabela) = tabelas
        .iter()
        .find(|t| t.ativa && t.tipo == TipoTabela::Venda)
    else {
        return 0;
    };
    let lista: Result<Vec<PrecoDefasado>, _> = motor.consultar(
        sessao,
        "vendas.precos_defasados.v1",
        &PrecosDefasados {
            tabela_preco: tabela.id,
        },
    );
    lista.map_or(0, |l| l.iter().filter(|d| d.situacao.e_defasado()).count())
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::testes_comum::motor_de_teste;

    #[test]
    fn so_avisa_quando_ha_defasado() {
        assert_eq!(texto_do_aviso(0), None);
        assert_eq!(
            texto_do_aviso(3).as_deref(),
            Some("3 preço(s) ficaram defasados com esta compra — veja Estoque › Preços")
        );
    }

    #[test]
    fn sem_tabela_de_venda_nao_avisa() {
        let t = motor_de_teste();
        assert_eq!(contar_defasados(&t.motor, &t.sessao), 0);
    }
}
