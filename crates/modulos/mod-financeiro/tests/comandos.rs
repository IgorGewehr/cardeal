//! Os comandos do financeiro atravessando o `Despachante` real contra SQLite: autorização,
//! transação, receituário no Razão e persistência — tudo junto.

#![allow(clippy::result_large_err)] // `ErroArmazenamento` carrega detalhes de propósito

use cardeal_auth::{AutorizacoesEfetivas, EmissaoSessao, Escopo, Papel, Sessao};
use cardeal_kernel::{CodigoErro, Data, Dinheiro, Fuso, Id, Instante};
use cardeal_ledger::semear_plano_padrao;
use cardeal_modkit::{Ambiente, Despachante, Modulo, PedidoAtivacao, RegistroModulos};
use cardeal_storage::{Armazenamento, ConfigArmazenamento, ContextoEscrita, ErroArmazenamento};
use mod_financeiro::{
    BaixarRecebimento, LancarTituloAReceber, ModuloFinanceiro, RecebimentoBaixado,
    TituloAReceberLancado, MANIFESTO,
};
use serde::Serialize;
use tempfile::TempDir;

fn liga(e: cardeal_ledger::ErroRazao) -> ErroArmazenamento {
    ErroArmazenamento::Sqlite(e.to_string())
}

fn base() -> (TempDir, Armazenamento, Id) {
    let dir = tempfile::tempdir().unwrap();
    let arm =
        Armazenamento::abrir(ConfigArmazenamento::arquivo(dir.path().join("cardeal.db"))).unwrap();
    arm.migrar(&[
        cardeal_ledger::migracoes::conjunto(),
        ModuloFinanceiro.migracoes(),
    ])
    .unwrap();

    let empresa = Id::novo();
    let ctx = ContextoEscrita::novo(empresa, Id::novo(), Id::novo(), Id::novo());
    arm.escritor()
        .executar(ctx, move |uow| {
            uow.conexao()
                .execute(
                    "INSERT INTO nucleo_empresa
                       (id, razao_social, nome_fantasia, cnpj, regime, endereco, perfil, criado_em)
                     VALUES (?1,'Teste LTDA','Teste','11222333000181','SimplesNacional','{}','comercio',0)",
                    [empresa.em_bytes().as_slice()],
                )
                .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))?;
            semear_plano_padrao(uow, empresa).map_err(liga)
        })
        .unwrap();

    (dir, arm, empresa)
}

fn ambiente(empresa: Id) -> Ambiente {
    let mut rm = RegistroModulos::novo();
    rm.registrar(&MANIFESTO).unwrap();
    let efetivo = rm
        .resolver(&PedidoAtivacao::nova().com_modulo("financeiro"))
        .unwrap();
    Ambiente::novo(empresa, efetivo)
}

fn sessao(empresa: Id, permissoes: &[&str]) -> Sessao {
    let mut papel = Papel::novo(empresa, "Financeiro");
    for p in permissoes {
        papel = papel.com_permissao(*p);
    }
    Sessao::abrir(EmissaoSessao::padrao(
        Id::novo(),
        Id::novo(),
        Escopo::empresa_inteira(empresa),
        AutorizacoesEfetivas::consolidar([&papel]),
        Instante::EPOCA,
    ))
}

fn hoje() -> Data {
    Data::hoje(Fuso::BRASILIA)
}

fn carga(v: &impl Serialize) -> Vec<u8> {
    postcard::to_stdvec(v).unwrap()
}

fn conta_lancamentos(arm: &Armazenamento, empresa: Id) -> i64 {
    arm.leitor()
        .consultar(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM razao_lancamento WHERE empresa = ?1",
                [empresa.em_bytes().as_slice()],
                |r| r.get(0),
            )
            .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
        })
        .unwrap()
}

fn conta_baixas(arm: &Armazenamento, empresa: Id) -> i64 {
    arm.leitor()
        .consultar(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM financeiro_baixa WHERE empresa = ?1",
                [empresa.em_bytes().as_slice()],
                |r| r.get(0),
            )
            .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
        })
        .unwrap()
}

fn lancar(
    d: &Despachante,
    arm: &Armazenamento,
    empresa: Id,
    s: &Sessao,
    valor: Dinheiro,
    parcelas: u16,
) -> TituloAReceberLancado {
    let cmd = LancarTituloAReceber {
        cliente: Id::novo(),
        valor_total: valor,
        emissao: hoje(),
        parcelas,
        primeiro_vencimento: hoje(),
        intervalo_dias: 30,
        observacao: None,
    };
    let saida = d
        .executar_comando(
            "financeiro.lancar_titulo_a_receber.v1",
            &carga(&cmd),
            s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap();
    postcard::from_bytes(&saida).unwrap()
}

#[test]
fn lancar_titulo_cria_parcelas_e_um_lancamento_confirmado_por_parcela() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloFinanceiro]).unwrap();
    let s = sessao(empresa, &["financeiro.receber.criar"]);

    let out = lancar(&d, &arm, empresa, &s, Dinheiro::reais(90), 3);
    assert_eq!(out.parcelas.len(), 3);
    assert_eq!(out.lancamentos.len(), 3);
    assert_eq!(conta_lancamentos(&arm, empresa), 3);
}

#[test]
fn baixa_em_dia_quita_a_parcela_e_gera_lancamento_realizado() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloFinanceiro]).unwrap();
    let s = sessao(
        empresa,
        &["financeiro.receber.criar", "financeiro.receber.baixar"],
    );

    let titulo = lancar(&d, &arm, empresa, &s, Dinheiro::reais(100), 1);
    let cmd = BaixarRecebimento {
        parcela: titulo.parcelas[0],
        valor: Dinheiro::reais(100),
        data: hoje(),
        conta_destino: None,
    };
    let saida = d
        .executar_comando(
            "financeiro.baixar_recebimento.v1",
            &carga(&cmd),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap();
    let baixada: RecebimentoBaixado = postcard::from_bytes(&saida).unwrap();

    assert!(baixada.parcela_quitada);
    assert_eq!(baixada.saldo_restante, Dinheiro::ZERO);
    assert_eq!(conta_baixas(&arm, empresa), 1);
    // 1 lançamento Confirmado (título) + 1 Realizado (baixa).
    assert_eq!(conta_lancamentos(&arm, empresa), 2);
}

#[test]
fn baixa_parcial_deixa_saldo_e_nao_quita() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloFinanceiro]).unwrap();
    let s = sessao(
        empresa,
        &["financeiro.receber.criar", "financeiro.receber.baixar"],
    );

    let titulo = lancar(&d, &arm, empresa, &s, Dinheiro::reais(100), 1);
    let cmd = BaixarRecebimento {
        parcela: titulo.parcelas[0],
        valor: Dinheiro::reais(40),
        data: hoje(),
        conta_destino: None,
    };
    let saida = d
        .executar_comando(
            "financeiro.baixar_recebimento.v1",
            &carga(&cmd),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap();
    let baixada: RecebimentoBaixado = postcard::from_bytes(&saida).unwrap();
    assert!(!baixada.parcela_quitada);
    assert_eq!(baixada.saldo_restante, Dinheiro::reais(60));
}

#[test]
fn sem_a_permissao_de_criar_o_lancamento_e_recusado() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloFinanceiro]).unwrap();
    let s = sessao(empresa, &["financeiro.receber.ver"]); // não pode criar

    let cmd = LancarTituloAReceber {
        cliente: Id::novo(),
        valor_total: Dinheiro::reais(10),
        emissao: hoje(),
        parcelas: 1,
        primeiro_vencimento: hoje(),
        intervalo_dias: 30,
        observacao: None,
    };
    let erro = d
        .executar_comando(
            "financeiro.lancar_titulo_a_receber.v1",
            &carga(&cmd),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap_err();
    assert_eq!(erro.codigo, CodigoErro::SEM_PERMISSAO);
    assert_eq!(conta_lancamentos(&arm, empresa), 0);
}

#[test]
fn baixa_acima_do_devido_falha_e_desfaz_tudo() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloFinanceiro]).unwrap();
    let s = sessao(
        empresa,
        &["financeiro.receber.criar", "financeiro.receber.baixar"],
    );

    let titulo = lancar(&d, &arm, empresa, &s, Dinheiro::reais(100), 1);
    let antes = conta_lancamentos(&arm, empresa);

    let cmd = BaixarRecebimento {
        parcela: titulo.parcelas[0],
        valor: Dinheiro::reais(150),
        data: hoje(),
        conta_destino: None,
    };
    let erro = d
        .executar_comando(
            "financeiro.baixar_recebimento.v1",
            &carga(&cmd),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap_err();
    assert_eq!(erro.codigo, CodigoErro::REGRA_VIOLADA);
    // Nada foi gravado: nem o lançamento Realizado, nem a baixa.
    assert_eq!(conta_lancamentos(&arm, empresa), antes);
    assert_eq!(conta_baixas(&arm, empresa), 0);
}
