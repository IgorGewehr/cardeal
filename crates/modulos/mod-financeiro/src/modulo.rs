//! O `impl Modulo` do financeiro — o ponto por onde o motor coleta manifesto, migrações e
//! manipuladores (`docs/contratos-internos.md` §4). Sem lógica: só amarração.

use cardeal_kernel::Resultado;
use cardeal_modkit::{Manifesto, Modulo, Registro};
use cardeal_storage::ConjuntoMigracoes;

use crate::comandos::{
    AbrirCaixa, BaixarPagamento, BaixarRecebimento, CadastrarCaixa, CriarCategoria,
    CriarContaBancaria, CriarRecorrencia, EstornarBaixa, FecharCaixa, LancarTituloAPagar,
    LancarTituloAReceber, RegistrarSangria, RegistrarSuprimento, RenegociarTitulo,
};
use crate::consultas::{
    Caixas, Categorias, ContasDeCaixa, ContasDeResultado, ContasDisponiveis, ExtratoDisponivel,
    Recorrencias, TitulosAPagarEmAberto, TitulosAReceberEmAberto, TotalPorCategoriaNoPeriodo,
};
use crate::manifesto::MANIFESTO;
use crate::migracoes;

/// O módulo financeiro, para registrar no [`Despachante`](cardeal_modkit::Despachante).
pub struct ModuloFinanceiro;

impl Modulo for ModuloFinanceiro {
    fn manifesto(&self) -> &'static Manifesto {
        &MANIFESTO
    }

    fn migracoes(&self) -> ConjuntoMigracoes {
        migracoes::conjunto()
    }

    fn registrar(&self, registro: &mut Registro) -> Resultado<()> {
        registro
            .comando::<LancarTituloAReceber>("financeiro.lancar_titulo_a_receber.v1")
            .comando::<LancarTituloAPagar>("financeiro.lancar_titulo_a_pagar.v1")
            .comando::<BaixarRecebimento>("financeiro.baixar_recebimento.v1")
            .comando::<BaixarPagamento>("financeiro.baixar_pagamento.v1")
            .comando::<CadastrarCaixa>("financeiro.cadastrar_caixa.v1")
            .comando::<AbrirCaixa>("financeiro.abrir_caixa.v1")
            .comando::<RegistrarSuprimento>("financeiro.registrar_suprimento.v1")
            .comando::<RegistrarSangria>("financeiro.registrar_sangria.v1")
            .comando::<FecharCaixa>("financeiro.fechar_caixa.v1")
            .comando::<EstornarBaixa>("financeiro.estornar_baixa.v1")
            .comando::<RenegociarTitulo>("financeiro.renegociar_titulo.v1")
            .comando::<CriarContaBancaria>("financeiro.criar_conta_bancaria.v1")
            .comando::<CriarCategoria>("financeiro.criar_categoria.v1")
            .comando::<CriarRecorrencia>("financeiro.criar_recorrencia.v1")
            .consulta::<TitulosAReceberEmAberto>("financeiro.titulos_a_receber_em_aberto.v1")
            .consulta::<TitulosAPagarEmAberto>("financeiro.titulos_a_pagar_em_aberto.v1")
            .consulta::<Categorias>("financeiro.categorias.v1")
            .consulta::<TotalPorCategoriaNoPeriodo>("financeiro.total_por_categoria_no_periodo.v1")
            .consulta::<ContasDeResultado>("financeiro.contas_de_resultado.v1")
            .consulta::<Recorrencias>("financeiro.recorrencias.v1")
            .consulta::<Caixas>("financeiro.caixas.v1")
            .consulta::<ContasDeCaixa>("financeiro.contas_caixa.v1")
            .consulta::<ContasDisponiveis>("financeiro.contas_disponiveis.v1")
            .consulta::<ExtratoDisponivel>("financeiro.extrato_disponivel.v1");
        Ok(())
    }
}
