//! A carteira de cheques pelo `Despachante`: receber (baixa em Cheques a receber), compensar
//! (D Banco / C Cheques a receber, conciliável com o extrato) e devolver (estorna compensação e
//! baixa, a parcela reabre).

#![allow(clippy::result_large_err)]

use cardeal_auth::{AutorizacoesEfetivas, EmissaoSessao, Escopo, Papel, Sessao};
use cardeal_kernel::{CodigoErro, Data, Dinheiro, Fuso, Id, Instante};
use cardeal_ledger::semear_plano_padrao;
use cardeal_modkit::{Ambiente, Despachante, Modulo, PedidoAtivacao, RegistroModulos};
use cardeal_storage::{Armazenamento, ConfigArmazenamento, ContextoEscrita, ErroArmazenamento};
use mod_financeiro::cheque::{DadosDoCheque, EstadoCheque};
use mod_financeiro::extrato::FormatoExtrato;
use mod_financeiro::sugestao::AlvoDaConciliacao;
use mod_financeiro::{
    CarteiraDeCheques, ChequeCompensado, ChequeFoiDevolvido, ChequeRegistrado, CompensarCheque,
    ConciliarItem, ContaBancariaCriada, CriarContaBancaria, DevolverCheque, ExtratoImportado,
    ImportarExtrato, ItemCheque, ItemDoExtrato, ItensDoExtrato, LancarTituloAReceber,
    ModuloFinanceiro, RegistrarCheque, TituloAReceberLancado, MANIFESTO,
};
use serde::de::DeserializeOwned;
use serde::Serialize;
use tempfile::TempDir;

struct Cena {
    _dir: TempDir,
    arm: Armazenamento,
    empresa: Id,
    d: Despachante,
    s: Sessao,
    amb: Ambiente,
}

fn hoje() -> Data {
    Data::hoje(Fuso::BRASILIA)
}

fn cena() -> Cena {
    let dir = tempfile::tempdir().unwrap();
    let arm =
        Armazenamento::abrir(ConfigArmazenamento::arquivo(dir.path().join("cardeal.db"))).unwrap();
    arm.migrar(&[
        cardeal_ledger::migracoes::conjunto(),
        ModuloFinanceiro.migracoes(),
    ])
    .unwrap();
    let empresa = Id::novo();
    arm.escritor()
        .executar(
            ContextoEscrita::novo(empresa, Id::novo(), Id::novo(), Id::novo()),
            move |uow| {
                uow.conexao()
                    .execute(
                        "INSERT INTO nucleo_empresa
                           (id, razao_social, nome_fantasia, cnpj, regime, endereco, perfil, criado_em)
                         VALUES (?1,'Teste LTDA','Teste','11222333000181','SimplesNacional','{}','comercio',0)",
                        [empresa.em_bytes().as_slice()],
                    )
                    .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))?;
                semear_plano_padrao(uow, empresa)
                    .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
            },
        )
        .unwrap();
    let mut papel = Papel::novo(empresa, "Financeiro");
    for p in [
        "financeiro.banco.criar",
        "financeiro.receber.criar",
        "financeiro.receber.ver",
        "financeiro.cheque.ver",
        "financeiro.cheque.registrar",
        "financeiro.cheque.compensar",
        "financeiro.cheque.devolver",
        "financeiro.conciliacao.ver",
        "financeiro.conciliacao.importar",
        "financeiro.conciliacao.confirmar",
    ] {
        papel = papel.com_permissao(p);
    }
    let s = Sessao::abrir(EmissaoSessao::padrao(
        Id::novo(),
        Id::novo(),
        Escopo::empresa_inteira(empresa),
        AutorizacoesEfetivas::consolidar([&papel]),
        Instante::EPOCA,
    ));
    let mut rm = RegistroModulos::novo();
    rm.registrar(&MANIFESTO).unwrap();
    let efetivo = rm
        .resolver(
            &PedidoAtivacao::nova()
                .com_modulo("financeiro")
                .com_submodulo("financeiro", "cheques"),
        )
        .unwrap();
    Cena {
        _dir: dir,
        arm,
        empresa,
        d: Despachante::construir(&[&ModuloFinanceiro]).unwrap(),
        s,
        amb: Ambiente::novo(empresa, efetivo),
    }
}

impl Cena {
    fn tenta(&self, nome: &str, c: &impl Serialize) -> Result<Vec<u8>, cardeal_kernel::Erro> {
        self.d.executar_comando(
            nome,
            &postcard::to_stdvec(c).unwrap(),
            &self.s,
            &self.amb,
            self.arm.escritor(),
        )
    }

    fn cmd<T: DeserializeOwned>(&self, nome: &str, c: &impl Serialize) -> T {
        postcard::from_bytes(&self.tenta(nome, c).unwrap()).unwrap()
    }

    fn consulta<T: DeserializeOwned>(&self, nome: &str, c: &impl Serialize) -> T {
        let b = self
            .d
            .executar_consulta(
                nome,
                &postcard::to_stdvec(c).unwrap(),
                &self.s,
                &self.amb,
                self.arm.leitor(),
            )
            .unwrap();
        postcard::from_bytes(&b).unwrap()
    }

    fn escalar(&self, sql: &str) -> i64 {
        let (empresa, sql) = (self.empresa, sql.to_owned());
        self.arm
            .leitor()
            .consultar(move |c| {
                c.query_row(&sql, [empresa.em_bytes().as_slice()], |r| r.get(0))
                    .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
            })
            .unwrap()
    }

    /// O saldo (débitos − créditos) de uma conta pelo código, em reais.
    fn saldo(&self, codigo: &str) -> i64 {
        self.escalar(&format!(
            "SELECT COALESCE(SUM(rp.valor), 0) / 100 FROM razao_partida rp
             JOIN razao_conta rc ON rc.id = rp.conta
             JOIN razao_lancamento rl ON rl.id = rp.lancamento
             WHERE rc.empresa = ?1 AND rc.codigo = '{codigo}'
               AND rl.estado IN ('Confirmado','Realizado','Estornado')"
        ))
    }

    fn conta(&self, codigo: &str) -> Id {
        let (empresa, codigo) = (self.empresa, codigo.to_owned());
        let b: Vec<u8> = self
            .arm
            .leitor()
            .consultar(move |c| {
                c.query_row(
                    "SELECT id FROM razao_conta WHERE empresa = ?1 AND codigo = ?2",
                    rusqlite::params![empresa.em_bytes().as_slice(), codigo],
                    |r| r.get(0),
                )
                .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
            })
            .unwrap();
        Id::de_bytes(b.as_slice().try_into().unwrap())
    }

    fn parcela_a_receber(&self, reais: i64) -> Id {
        let t: TituloAReceberLancado = self.cmd(
            "financeiro.lancar_titulo_a_receber.v1",
            &LancarTituloAReceber {
                cliente: None,
                valor_total: Dinheiro::reais(reais),
                emissao: hoje(),
                parcelas: 1,
                primeiro_vencimento: hoje(),
                intervalo_dias: 0,
                observacao: Some("Venda no balcão".into()),
                categoria: None,
                quitado_agora: None,
            },
        );
        t.parcelas[0]
    }

    fn carteira(&self) -> Vec<ItemCheque> {
        self.consulta(
            "financeiro.carteira_de_cheques.v1",
            &CarteiraDeCheques { estado: None },
        )
    }
}

fn dados() -> DadosDoCheque {
    DadosDoCheque {
        banco: "001".into(),
        agencia: "1234".into(),
        conta: "56789".into(),
        numero: "000123".into(),
        emitente: "João Silva".into(),
        documento_emitente: None,
    }
}

#[test]
fn receber_compensar_conciliar_e_devolver_um_cheque() {
    let c = cena();
    let banco: ContaBancariaCriada = c.cmd(
        "financeiro.criar_conta_bancaria.v1",
        &CriarContaBancaria {
            nome: "Sicredi".into(),
        },
    );
    let parcela = c.parcela_a_receber(300);

    // Recebe um cheque pré-datado de 300: a parcela quita, o dinheiro ainda não entrou.
    let r: ChequeRegistrado = c.cmd(
        "financeiro.registrar_cheque.v1",
        &RegistrarCheque {
            parcela,
            valor: Dinheiro::reais(300),
            recebido_em: hoje(),
            bom_para: hoje().mais_dias(10),
            dados: dados(),
        },
    );
    assert!(r.parcela_quitada);
    assert_eq!(c.saldo("1.2.03"), 300, "em Cheques a receber");
    assert_eq!(c.saldo("1.2.01"), 0, "o cliente já não deve");
    assert_eq!(
        c.saldo("1.1.01") + c.saldo("1.1.02"),
        0,
        "caixa e banco intocados"
    );
    let carteira = c.carteira();
    assert_eq!(carteira.len(), 1);
    assert_eq!(carteira[0].cheque.estado, EstadoCheque::EmCarteira);
    assert_eq!(carteira[0].descricao.as_deref(), Some("Venda no balcão"));
    assert!(!carteira[0].cheque.pronto_para_depositar(hoje()));

    // O mesmo cheque de novo (noutra parcela): recusado.
    let outra = c.parcela_a_receber(300);
    let e = c
        .tenta(
            "financeiro.registrar_cheque.v1",
            &RegistrarCheque {
                parcela: outra,
                valor: Dinheiro::reais(300),
                recebido_em: hoje(),
                bom_para: hoje(),
                dados: dados(),
            },
        )
        .unwrap_err();
    assert_eq!(e.codigo, CodigoErro::REGRA_VIOLADA);

    // Compensar em Cheques a receber (não é banco) ou no futuro: recusado.
    for (conta, data) in [
        (c.conta("1.2.03"), hoje()),
        (banco.conta, hoje().mais_dias(1)),
    ] {
        let e = c
            .tenta(
                "financeiro.compensar_cheque.v1",
                &CompensarCheque {
                    cheque: r.cheque,
                    conta,
                    data,
                },
            )
            .unwrap_err();
        assert_eq!(e.codigo, CodigoErro::ENTRADA_INVALIDA);
    }
    let comp: ChequeCompensado = c.cmd(
        "financeiro.compensar_cheque.v1",
        &CompensarCheque {
            cheque: r.cheque,
            conta: banco.conta,
            data: hoje(),
        },
    );
    assert_eq!(c.saldo("1.2.03"), 0);
    assert_eq!(c.carteira()[0].cheque.estado, EstadoCheque::Compensado);

    // O depósito aparece no extrato e casa com a compensação.
    let d = hoje().formatar_iso().replace('-', "");
    let importar = |fitid: &str, valor: &str, memo: &str| {
        let _: ExtratoImportado = c.cmd(
            "financeiro.importar_extrato.v1",
            &ImportarExtrato {
                conta: banco.conta,
                formato: FormatoExtrato::Ofx,
                arquivo: "extrato.ofx".into(),
                conteudo: format!(
                    "<OFX><STMTTRN><DTPOSTED>{d}<TRNAMT>{valor}<FITID>{fitid}<MEMO>{memo}\n</STMTTRN></OFX>"
                ),
            },
        );
        let itens: Vec<ItemDoExtrato> = c.consulta(
            "financeiro.itens_do_extrato.v1",
            &ItensDoExtrato {
                conta: banco.conta,
                estado: None,
            },
        );
        itens.into_iter().find(|i| i.descricao == memo).unwrap()
    };
    let deposito = importar("C1", "300.00", "DEP CHEQUE");
    assert_eq!(
        deposito.sugestoes[0].alvo,
        AlvoDaConciliacao::Movimento(comp.lancamento)
    );
    let _: () = c.cmd(
        "financeiro.conciliar_item.v1",
        &ConciliarItem {
            item: deposito.id,
            alvo: deposito.sugestoes[0].alvo,
        },
    );

    // Voltou sem fundos: estorna a compensação e a baixa; a parcela reabre.
    let dev: ChequeFoiDevolvido = c.cmd(
        "financeiro.devolver_cheque.v1",
        &DevolverCheque {
            cheque: r.cheque,
            alinea: Some(11),
            observacao: "cliente avisado".into(),
        },
    );
    assert_eq!(dev.parcela, parcela);
    assert!(dev.estorno_compensacao.is_some());
    // A parcela reaberta (300) mais a outra, que segue em aberto desde o teste do duplicado.
    assert_eq!(c.saldo("1.2.01"), 600, "o cliente volta a dever");
    assert_eq!(c.saldo("1.2.03"), 0);
    assert_eq!(
        c.escalar(&format!(
            "SELECT COALESCE(SUM(rp.valor), 0) / 100 FROM razao_partida rp
         JOIN razao_lancamento rl ON rl.id = rp.lancamento
         WHERE rl.empresa = ?1 AND rp.conta = x'{}'",
            banco
                .conta
                .em_bytes()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        )),
        0,
        "o banco devolveu o dinheiro"
    );
    let aberta = c.escalar(&format!(
        "SELECT COUNT(*) FROM financeiro_parcela WHERE empresa = ?1 AND estado = 'Aberta'
           AND id = x'{}'",
        parcela
            .em_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    ));
    assert_eq!(aberta, 1);
    let devolvido = &c.carteira()[0].cheque;
    assert_eq!(devolvido.estado, EstadoCheque::Devolvido);
    assert_eq!(devolvido.alinea, Some(11));
    assert!(devolvido
        .motivo
        .as_deref()
        .is_some_and(|m| m.contains("Sem fundos") && m.contains("cliente avisado")));
    assert_eq!(
        c.escalar(
            "SELECT COUNT(*) FROM nucleo_outbox
             WHERE tipo = 'financeiro.cheque_devolvido.v1' AND ?1 IS NOT NULL"
        ),
        1
    );
    // O débito da devolução no extrato casa com o estorno da compensação.
    let volta = importar("C2", "-300.00", "CHEQUE DEVOLVIDO");
    assert!(matches!(
        volta.sugestoes.first().map(|s| s.alvo),
        Some(AlvoDaConciliacao::Movimento(_))
    ));

    // Devolver de novo: recusado. Reapresentar o mesmo cheque: aceito.
    let e = c
        .tenta(
            "financeiro.devolver_cheque.v1",
            &DevolverCheque {
                cheque: r.cheque,
                alinea: None,
                observacao: String::new(),
            },
        )
        .unwrap_err();
    assert_eq!(e.codigo, CodigoErro::ESTADO_INVALIDO);
    let _: ChequeRegistrado = c.cmd(
        "financeiro.registrar_cheque.v1",
        &RegistrarCheque {
            parcela,
            valor: Dinheiro::reais(300),
            recebido_em: hoje(),
            bom_para: hoje(),
            dados: dados(),
        },
    );
    assert_eq!(c.carteira().len(), 2);
}
